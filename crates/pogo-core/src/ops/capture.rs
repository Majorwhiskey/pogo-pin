//! `serial capture` as an op; also the capture step of `run`.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::outcome::{outcome_error, symbolize_panic};
use super::target::{Selection, Target, acquire};
use super::{DEFAULT_CAPTURE_TIMEOUT, OpResult, usage};
use crate::config::{ParserName, ResetMode as ConfigResetMode};
use crate::deadline::Deadline;
use crate::envelope::ErrorBody;
use crate::error::PogoError;
use crate::serial::capture::CaptureOptions;
use crate::serial::session::{
    SerialCaptureData, SessionOptions, SessionOutput, capture_device, capture_on, compile_patterns,
};
use crate::serial::{DecodeOptions, LogLine};
use crate::transport::ReplayTransport;

/// Everything `serial capture` can be asked to do (docs/commands.md §7.6).
#[derive(Debug, Clone)]
pub struct CaptureRequest {
    pub baud: Option<u32>,
    pub until: Vec<String>,
    pub fail_on: Vec<String>,
    pub no_default_fail_patterns: bool,
    pub settle: Option<Duration>,
    pub max_lines: usize,
    pub max_line_len: usize,
    /// `None`: the device's configured parser, else `auto`.
    pub parser: Option<ParserName>,
    pub elf: Option<PathBuf>,
    pub raw: bool,
    pub record: Option<PathBuf>,
    pub reconnect_timeout: Duration,
}

impl Default for CaptureRequest {
    fn default() -> Self {
        CaptureRequest {
            baud: None,
            until: Vec::new(),
            fail_on: Vec::new(),
            no_default_fail_patterns: false,
            settle: None,
            max_lines: 500,
            max_line_len: 1000,
            parser: None,
            elf: None,
            raw: false,
            record: None,
            reconnect_timeout: Duration::from_secs(5),
        }
    }
}

/// Things the adapter supplies: how to stream lines and how to stop early.
pub struct Live<'a> {
    pub on_line: &'a mut dyn FnMut(&LogLine),
    pub cancel: Option<Arc<AtomicBool>>,
}

/// `timeout`: `None` means the config's `defaults.timeout`, else 10 s.
pub fn serial_capture(
    sel: &Selection,
    req: &CaptureRequest,
    reset: bool,
    timeout: Option<Duration>,
    live: Live,
) -> OpResult<SerialCaptureData> {
    if let Err(e) = check_patterns(req) {
        return OpResult::fail(e);
    }
    let target = match acquire(sel, "serial.capture") {
        Ok(t) => t,
        Err(e) => return OpResult::fail(e),
    };
    let timeout = target.timeout(timeout, DEFAULT_CAPTURE_TIMEOUT);
    match capture_locked(&target, req, reset, timeout, live) {
        Ok(done) => {
            let error = outcome_error(&done.out.data.capture, &req.until, timeout, done.reconnect);
            OpResult::ok(done.out.data)
                .with_warnings(done.out.warnings)
                .with_error(error)
        }
        Err(e) => OpResult::fail(e),
    }
}

/// Bad patterns are usage errors, reported before any device is touched.
pub fn check_patterns(req: &CaptureRequest) -> Result<(), ErrorBody> {
    compile_patterns(&req.until, "until")
        .and_then(|_| compile_patterns(&req.fail_on, "fail_on"))
        .map(|_| ())
        .map_err(usage)
}

pub struct CaptureDone {
    pub out: SessionOutput,
    /// How long a vanished device was waited for, when reconnecting applied.
    pub reconnect: Option<Duration>,
}

/// Capture from a locked target. The timeout starts here, so time spent waiting for the
/// lock is not charged to it.
pub fn capture_locked(
    target: &Target,
    req: &CaptureRequest,
    reset: bool,
    timeout: Duration,
    live: Live,
) -> Result<CaptureDone, ErrorBody> {
    if reset && target.device_config().and_then(|d| d.reset) == Some(ConfigResetMode::Probe) {
        return Err(PogoError::NotImplemented("reset via probe").into());
    }
    let deadline = Deadline::after(timeout);
    let until = compile_patterns(&req.until, "until").map_err(usage)?;
    let fail_on = target.fail_on(!req.no_default_fail_patterns, &req.fail_on);
    let fail_on = compile_patterns(&fail_on, "fail_on").map_err(usage)?;
    let record: Option<Box<dyn Write + Send>> = match &req.record {
        None => None,
        Some(path) => {
            let f = std::fs::File::create(path)
                .map_err(|e| usage(format!("cannot create record file {}: {e}", path.display())))?;
            Some(Box::new(std::io::BufWriter::new(f)))
        }
    };
    let opts = CaptureOptions {
        until,
        fail_on,
        settle: req.settle.unwrap_or(Duration::ZERO),
        max_lines: req.max_lines,
        decode: DecodeOptions {
            parser: req
                .parser
                .or(target.device_config().and_then(|d| d.parser))
                .unwrap_or(ParserName::Auto),
            keep_ansi: req.raw,
            max_line_len: req.max_line_len,
        },
        expect_one_boot: reset,
        cancel: live.cancel,
    };
    let session = SessionOptions {
        baud: target.baud(req.baud),
        reset,
        record,
        reconnect: req.reconnect_timeout,
    };
    let device = target.device.clone();
    let reconnect = (target.replay.is_none()
        && device.usb_serial.is_some()
        && !req.reconnect_timeout.is_zero())
    .then_some(req.reconnect_timeout);
    let on_line = live.on_line;
    let mut out = match &target.replay {
        Some(path) => ReplayTransport::from_file(path)
            .and_then(|t| capture_on(t, device, session, &opts, deadline, on_line)),
        None => capture_device(device, session, &opts, deadline, on_line),
    }?;
    let elf = target.elf(req.elf.as_deref());
    out.warnings
        .extend(symbolize_panic(&mut out.data.capture, elf.as_deref()));
    Ok(CaptureDone { out, reconnect })
}
