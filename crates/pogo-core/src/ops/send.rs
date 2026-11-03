//! `serial send` as an op.

use std::path::PathBuf;
use std::time::Duration;

use super::capture::Live;
use super::outcome::{outcome_error, symbolize_panic};
use super::target::{Selection, Target, acquire};
use super::{DEFAULT_SEND_TIMEOUT, OpResult, usage};
use crate::audit::{self, AuditEntry};
use crate::config::ParserName;
use crate::deadline::Deadline;
use crate::envelope::{ErrorBody, Warning};
use crate::serial::DecodeOptions;
use crate::serial::capture::CaptureOptions;
use crate::serial::send::{SerialSendData, send_device, send_on};
use crate::serial::session::compile_patterns;
use crate::transport::ReplayTransport;

#[derive(Debug, Clone)]
pub struct SendRequest {
    /// Exact bytes to write, line ending included.
    pub bytes: Vec<u8>,
    /// Capture until a line matches this; no capture without it.
    pub expect: Option<String>,
    /// ELF to symbolize a panic in the reply (default: the device's configured `elf`).
    pub elf: Option<PathBuf>,
}

/// `timeout`: `None` means the config's `defaults.timeout`, else 5 s.
pub fn serial_send(
    sel: &Selection,
    req: &SendRequest,
    timeout: Option<Duration>,
    live: Live,
) -> OpResult<SerialSendData> {
    let expect: Vec<String> = req.expect.iter().cloned().collect();
    if let Err(msg) = compile_patterns(&expect, "expect") {
        return OpResult::fail(usage(msg));
    }
    let target = match acquire(sel, "serial.send") {
        Ok(t) => t,
        Err(e) => return OpResult::fail(e),
    };
    let timeout = target.timeout(timeout, DEFAULT_SEND_TIMEOUT);
    let (data, warnings) = match send_locked(&target, req, timeout, live) {
        Ok(r) => r,
        Err((e, warnings)) => return OpResult::fail(e).with_warnings(warnings),
    };
    let error = data
        .response
        .as_ref()
        .and_then(|r| outcome_error(r, &expect, timeout, None));
    OpResult::ok(data).with_warnings(warnings).with_error(error)
}

/// Send to an already locked target: write, capture the reply if `expect` is set,
/// audit, and symbolize a panic. Outcomes stay data; the caller maps them to errors.
pub fn send_locked(
    target: &Target,
    req: &SendRequest,
    timeout: Duration,
    live: Live,
) -> Result<(SerialSendData, Vec<Warning>), (ErrorBody, Vec<Warning>)> {
    let expect: Vec<String> = req.expect.iter().cloned().collect();
    let until = compile_patterns(&expect, "expect").map_err(|m| (usage(m), Vec::new()))?;
    let deadline = Deadline::after(timeout);
    let fail_on = compile_patterns(&target.fail_on(true, &[]), "fail_on")
        .map_err(|m| (usage(m), Vec::new()))?;
    let opts = CaptureOptions {
        until,
        fail_on,
        settle: Duration::ZERO,
        max_lines: 500,
        decode: DecodeOptions {
            parser: ParserName::Auto,
            keep_ansi: false,
            max_line_len: 1000,
        },
        expect_one_boot: false,
        cancel: live.cancel,
    };
    let opts = req.expect.as_ref().map(|_| &opts);
    let device = target.device.clone();
    let result = match &target.replay {
        Some(path) => ReplayTransport::from_file(path)
            .and_then(|t| send_on(t, device, &req.bytes, opts, deadline, live.on_line)),
        None => send_device(
            device,
            target.baud(None),
            &req.bytes,
            opts,
            deadline,
            live.on_line,
        ),
    };
    let mut warnings = Vec::new();
    if target.replay.is_none() {
        let outcome = match &result {
            Ok(_) => "ok",
            Err(e) => e.code().as_str(),
        };
        // Log the payload size, not its contents: it may hold credentials.
        let args = serde_json::json!({ "bytes": req.bytes.len(), "expect": req.expect });
        let d = &target.device;
        let entry = AuditEntry::new(
            "serial.send",
            &d.port,
            d.usb_serial.as_deref(),
            args,
            outcome,
        );
        if let Err(e) = audit::record(&entry) {
            warnings.push(Warning::new("AUDIT_FAILED", e));
        }
    }
    let mut data = match result {
        Ok(d) => d,
        Err(e) => return Err((ErrorBody::from(e), warnings)),
    };
    let elf = target.elf(req.elf.as_deref());
    if let Some(r) = data.response.as_mut() {
        warnings.extend(symbolize_panic(r, elf.as_deref()));
    }
    Ok((data, warnings))
}
