//! `pogo serial send`.

use std::io::Read;
use std::time::Duration;

use pogo_core::audit::{self, AuditEntry};
use pogo_core::config::ParserName;
use pogo_core::deadline::Deadline;
use pogo_core::serial::capture::CaptureOptions;
use pogo_core::serial::send::{SerialSendData, parse_hex, send_device, send_on};
use pogo_core::serial::session::compile_patterns;
use pogo_core::serial::{DecodeOptions, LogLine};
use pogo_core::transport::ReplayTransport;
use pogo_core::{ErrorBody, ErrorCode, Warning};

use crate::cli::{GlobalArgs, LineEnding};
use crate::commands::serial::{outcome_error, stream_line, symbolize_panic};
use crate::commands::target;
use crate::interrupt::interrupt_flag;
use crate::output::{Format, Render, Reply, Style};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

pub struct SendArgs<'a> {
    pub data: &'a str,
    pub line_ending: LineEnding,
    pub hex: bool,
    pub expect: Option<&'a str>,
}

pub fn send(global: &GlobalArgs, args: SendArgs, format: Format, style: &Style) -> Reply {
    let timeout = global.timeout.unwrap_or(DEFAULT_TIMEOUT);
    let usage = |msg: String| Reply::error(ErrorBody::new(ErrorCode::Usage, msg));
    let mut bytes = match payload(&args) {
        Ok(b) => b,
        Err(msg) => return usage(msg),
    };
    bytes.extend_from_slice(match args.line_ending {
        LineEnding::Lf => b"\n",
        LineEnding::Crlf => b"\r\n",
        LineEnding::None => b"",
    });
    let expect: Vec<String> = args.expect.map(str::to_string).into_iter().collect();
    let until = match compile_patterns(&expect, "--expect") {
        Ok(u) => u,
        Err(msg) => return usage(msg),
    };

    let target = match target::acquire(global, "serial.send") {
        Ok(t) => t,
        Err(reply) => return *reply,
    };
    let deadline = Deadline::after(timeout);
    let fail_on = match compile_patterns(&target.fail_on(true, &[]), "--fail-on") {
        Ok(f) => f,
        Err(msg) => return usage(msg),
    };
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
        cancel: Some(interrupt_flag()),
    };
    let opts = args.expect.map(|_| &opts);
    let device = target.device.clone();
    let mut stream = |line: &LogLine| stream_line(format, line);
    let result = match &target.replay {
        Some(path) => ReplayTransport::from_file(path)
            .and_then(|t| send_on(t, device, &bytes, opts, deadline, &mut stream)),
        None => send_device(
            device,
            target.baud(None),
            &bytes,
            opts,
            deadline,
            &mut stream,
        ),
    };
    let audit_warning = target.replay.is_none().then(|| {
        let outcome = match &result {
            Ok(_) => "ok",
            Err(e) => e.code().as_str(),
        };
        // Log the payload size, not its contents: it may hold credentials.
        let args = serde_json::json!({ "bytes": bytes.len(), "expect": args.expect });
        let d = &target.device;
        let entry = AuditEntry::new(
            "serial.send",
            &d.port,
            d.usb_serial.as_deref(),
            args,
            outcome,
        );
        audit::record(&entry)
            .err()
            .map(|e| Warning::new("AUDIT_FAILED", e))
    });
    match result {
        Ok(mut data) => {
            let elf = target.elf(None);
            let warning = data
                .response
                .as_mut()
                .and_then(|r| symbolize_panic(r, elf.as_deref()));
            let err = data
                .response
                .as_ref()
                .and_then(|r| outcome_error(r, &expect, timeout, None));
            let mut reply = Reply::ok(&data, style);
            reply.warnings.extend(warning);
            reply.warnings.extend(audit_warning.flatten());
            match err {
                Some(e) => reply.with_error(e),
                None => reply,
            }
        }
        Err(e) => Reply::error(&e),
    }
}

/// The bytes to send before the line ending: the argument, or stdin for `-`.
fn payload(args: &SendArgs) -> Result<Vec<u8>, String> {
    let raw = if args.data == "-" {
        let mut buf = Vec::new();
        std::io::stdin()
            .read_to_end(&mut buf)
            .map_err(|e| format!("cannot read stdin: {e}"))?;
        buf
    } else {
        args.data.as_bytes().to_vec()
    };
    if args.hex {
        parse_hex(&String::from_utf8_lossy(&raw))
    } else {
        Ok(raw)
    }
}

impl Render for SerialSendData {
    fn render_human(&self, out: &mut String, style: &Style) {
        let mut summary = format!("sent {} bytes to {}", self.bytes_written, self.device.port);
        if let Some(m) = self.response.as_ref().and_then(|r| r.matched.as_ref()) {
            summary.push_str(&format!(
                "; reply matched /{}/ on line {} after {} ms",
                m.pattern, m.line_index, m.t_ms
            ));
        }
        out.push_str(&style.dim(&format!("-- {summary}")));
        out.push('\n');
    }
}
