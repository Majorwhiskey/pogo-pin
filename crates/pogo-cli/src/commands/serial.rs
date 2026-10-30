//! `pogo serial capture`.

use std::io::Write;
use std::time::Duration;

use pogo_core::config::ParserName;
use pogo_core::deadline::Deadline;
use pogo_core::serial::capture::{CaptureOptions, CaptureResult, FaultKind, Outcome};
use pogo_core::serial::session::{
    SerialCaptureData, SessionOptions, capture_device, capture_on, compile_patterns,
};
use pogo_core::serial::{DecodeOptions, LogLine};
use pogo_core::transport::ReplayTransport;
use pogo_core::{ErrorBody, ErrorCode};

use crate::cli::{CaptureArgs, GlobalArgs, LogParser};
use crate::commands::target;
use crate::interrupt::interrupt_flag;
use crate::output::{Format, Render, Reply, Style};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

pub fn capture(
    global: &GlobalArgs,
    reset: bool,
    args: &CaptureArgs,
    format: Format,
    style: &Style,
) -> Reply {
    let timeout = global.timeout.unwrap_or(DEFAULT_TIMEOUT);
    // Bad patterns are usage errors, reported before any device is touched.
    let until = match compile_patterns(&args.until, "--until")
        .and_then(|u| compile_patterns(&args.fail_on, "--fail-on").map(|_| u))
    {
        Ok(u) => u,
        Err(msg) => return Reply::error(ErrorBody::new(ErrorCode::Usage, msg)),
    };
    let target = match target::acquire(global, "serial.capture") {
        Ok(t) => t,
        Err(reply) => return *reply,
    };
    // --timeout covers the capture itself; time spent in --wait-lock is not charged to it.
    let deadline = Deadline::after(timeout);
    let fail_on = target.fail_on(!args.no_default_fail_patterns, &args.fail_on);
    let fail_on = match compile_patterns(&fail_on, "--fail-on") {
        Ok(f) => f,
        Err(msg) => return Reply::error(ErrorBody::new(ErrorCode::Usage, msg)),
    };

    let record: Option<Box<dyn Write + Send>> = match &args.record {
        None => None,
        Some(path) => match std::fs::File::create(path) {
            Ok(f) => Some(Box::new(std::io::BufWriter::new(f))),
            Err(e) => {
                let msg = format!("cannot create --record file {}: {e}", path.display());
                return Reply::error(ErrorBody::new(ErrorCode::Usage, msg));
            }
        },
    };

    let opts = CaptureOptions {
        until,
        fail_on,
        settle: args.settle.unwrap_or(Duration::ZERO),
        max_lines: args.max_lines,
        decode: DecodeOptions {
            parser: parser_name(args.parser),
            keep_ansi: args.raw,
            max_line_len: args.max_line_len,
        },
        expect_one_boot: reset,
        cancel: Some(interrupt_flag()),
    };
    let session = SessionOptions {
        baud: target.baud(args.baud),
        reset,
        record,
        reconnect: args.reconnect_timeout,
    };
    let device = target.device.clone();
    let reconnect = (target.replay.is_none()
        && device.usb_serial.is_some()
        && !args.reconnect_timeout.is_zero())
    .then_some(args.reconnect_timeout);
    let mut stream = |line: &LogLine| stream_line(format, line);
    let result = match &target.replay {
        Some(path) => ReplayTransport::from_file(path)
            .and_then(|t| capture_on(t, device, session, &opts, deadline, &mut stream)),
        None => capture_device(device, session, &opts, deadline, &mut stream),
    };
    match result {
        Ok(out) => {
            let mut reply = Reply::ok(&out.data, style);
            reply.warnings = out.warnings;
            match outcome_error(&out.data.capture, &args.until, timeout, reconnect) {
                Some(err) => reply.with_error(err),
                None => reply,
            }
        }
        Err(e) => Reply::error(&e),
    }
}

/// Lines go out as they arrive: plain text in human mode, `line` events in jsonl.
pub fn stream_line(format: Format, line: &LogLine) {
    let text = match format {
        Format::Human => format!("{}\n", line.text),
        Format::Jsonl => {
            let mut v = serde_json::to_value(line).unwrap_or_default();
            if let Some(obj) = v.as_object_mut() {
                obj.insert("type".into(), "line".into());
            }
            format!("{v}\n")
        }
        Format::Json => return,
    };
    let _ = std::io::stdout().lock().write_all(text.as_bytes());
}

fn parser_name(p: LogParser) -> ParserName {
    match p {
        LogParser::Auto => ParserName::Auto,
        LogParser::EspIdf => ParserName::EspIdf,
        LogParser::RustLog => ParserName::RustLog,
        LogParser::Raw => ParserName::Raw,
    }
}

/// Non-success outcomes become errors; the capture data stays in the envelope.
/// `reconnect` is the window pogo waited for a vanished device, when it tried.
pub fn outcome_error(
    c: &CaptureResult,
    until: &[String],
    timeout: Duration,
    reconnect: Option<Duration>,
) -> Option<ErrorBody> {
    let around = |i: usize| format!("See data.lines around line {i}.");
    match c.outcome {
        Outcome::Matched | Outcome::Completed => None,
        Outcome::Interrupted => Some(ErrorBody::new(
            ErrorCode::Interrupted,
            format!("interrupted after {} lines", c.line_count),
        )),
        Outcome::Timeout => Some(
            ErrorBody::new(
                ErrorCode::Timeout,
                format!(
                    "no line matched {} within {}",
                    until.join(" | "),
                    humantime::format_duration(timeout)
                ),
            )
            .with_hint("Raise --timeout, or check that the firmware prints the expected line."),
        ),
        Outcome::Eof => Some(
            ErrorBody::new(
                ErrorCode::DeviceIo,
                match reconnect {
                    Some(w) => format!(
                        "device disconnected during capture and did not come back within {}",
                        humantime::format_duration(w)
                    ),
                    None => "device disconnected during capture".to_string(),
                },
            )
            .with_hint("Check the USB cable; the board may have reset or been unplugged."),
        ),
        Outcome::Fault => {
            let fault = c.fault.as_ref()?;
            let msg = match (fault.kind, &c.panic) {
                (FaultKind::Panic, Some(p)) => {
                    let at = p
                        .location
                        .as_ref()
                        .map(|l| match l.column {
                            Some(col) => format!(" at {}:{}:{col}", l.file, l.line),
                            None => format!(" at {}:{}", l.file, l.line),
                        })
                        .unwrap_or_default();
                    let what = p
                        .message
                        .as_deref()
                        .map(|m| format!(": {m}"))
                        .unwrap_or_default();
                    format!("target panicked{at}{what}")
                }
                (FaultKind::UnexpectedReboot, _) => {
                    let reason = c.resets.last().map_or("", |r| r.reason.as_str());
                    format!("target rebooted unexpectedly ({reason})")
                }
                (_, _) => format!(
                    "line {} matched fail-on pattern '{}'",
                    fault.line_index,
                    fault.pattern.as_deref().unwrap_or_default()
                ),
            };
            Some(ErrorBody::new(ErrorCode::TargetFault, msg).with_hint(around(fault.line_index)))
        }
    }
}

impl Render for SerialCaptureData {
    /// Lines were already streamed; this is the closing summary.
    fn render_human(&self, out: &mut String, style: &Style) {
        let c = &self.capture;
        let summary = match &c.matched {
            Some(m) => format!(
                "matched /{}/ on line {} after {} ms",
                m.pattern, m.line_index, m.t_ms
            ),
            None => format!("captured {} lines", c.line_count),
        };
        out.push_str(&style.dim(&format!("-- {summary} ({})", self.device.port)));
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eof() -> CaptureResult {
        CaptureResult {
            outcome: Outcome::Eof,
            matched: None,
            fault: None,
            line_count: 0,
            omitted_lines: 0,
            boot_count: 0,
            resets: vec![],
            panic: None,
            lines: vec![],
        }
    }

    #[test]
    fn eof_message_says_how_long_pogo_waited() {
        let t = Duration::from_secs(10);
        let waited = outcome_error(&eof(), &[], t, Some(Duration::from_secs(5))).unwrap();
        assert_eq!(
            waited.message,
            "device disconnected during capture and did not come back within 5s"
        );
        let plain = outcome_error(&eof(), &[], t, None).unwrap();
        assert_eq!(plain.message, "device disconnected during capture");
        assert_eq!(plain.code, ErrorCode::DeviceIo);
    }
}
