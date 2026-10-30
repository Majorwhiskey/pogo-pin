//! Capture outcomes as errors, and panic symbolization, shared by every op that captures.

use std::path::Path;
use std::time::Duration;

use crate::envelope::{ErrorBody, Warning};
use crate::error::ErrorCode;
use crate::serial::capture::{CaptureResult, FaultKind, Outcome};
use crate::symbolize;

/// Non-success outcomes become errors; the capture data stays in the envelope.
/// `reconnect` is the window pogo waited for a vanished device, when it tried.
pub fn outcome_error(
    c: &CaptureResult,
    until: &[String],
    timeout: Duration,
    reconnect: Option<Duration>,
) -> Option<ErrorBody> {
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
            .with_hint("Raise the timeout, or check that the firmware prints the expected line."),
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
            let hint = format!("See data.lines around line {}.", fault.line_index);
            Some(ErrorBody::new(ErrorCode::TargetFault, msg).with_hint(hint))
        }
    }
}

/// Resolve a panic's backtrace against the ELF, if there is one. Failure is a warning:
/// the capture itself succeeded.
pub fn symbolize_panic(c: &mut CaptureResult, elf: Option<&Path>) -> Option<Warning> {
    let (Some(panic), Some(elf)) = (c.panic.as_mut(), elf) else {
        return None;
    };
    if panic.backtrace.is_empty() {
        return None;
    }
    symbolize::annotate(panic, elf)
        .err()
        .map(|e| Warning::new("SYMBOLIZE_FAILED", e))
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
