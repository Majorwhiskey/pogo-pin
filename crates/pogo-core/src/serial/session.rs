//! A whole `serial capture` against a resolved device: open the port, optionally record
//! and reset, then run the capture engine.

use std::io::Write;
use std::time::Duration;

use schemars::JsonSchema;
use serde::Serialize;

use super::LogLine;
use super::capture::{CaptureOptions, CaptureResult, capture};
use crate::deadline::Deadline;
use crate::discovery::{self, ResolvedDevice};
use crate::envelope::Warning;
use crate::error::PogoError;
use crate::reset::{hard_reset, is_usb_serial_jtag};
use crate::transport::{
    ReconnectingTransport, RecordingTransport, Reopen, SerialTransport, Transport,
};

/// `data` for `pogo serial capture`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SerialCaptureData {
    pub device: ResolvedDevice,
    #[serde(flatten)]
    pub capture: CaptureResult,
}

pub struct SessionOptions {
    pub baud: u32,
    /// Reset into run mode before capturing, so capture starts at the first boot byte.
    pub reset: bool,
    /// Where `--record` writes the session, already opened by the caller.
    pub record: Option<Box<dyn Write + Send>>,
    /// How long a vanished device may take to reappear (by USB serial) before the
    /// capture ends in `eof`. Zero disables reconnecting.
    pub reconnect: Duration,
}

pub struct SessionOutput {
    pub data: SerialCaptureData,
    pub warnings: Vec<Warning>,
}

/// Open the device's serial port and capture from it.
pub fn capture_device(
    device: ResolvedDevice,
    session: SessionOptions,
    opts: &CaptureOptions,
    deadline: Deadline,
    on_line: &mut dyn FnMut(&LogLine),
) -> Result<SessionOutput, PogoError> {
    let port = SerialTransport::open(&device.port, session.baud)?;
    match device.usb_serial.clone() {
        Some(serial) if !session.reconnect.is_zero() => {
            let reopen = reopen_by_serial(serial, session.baud);
            let t = ReconnectingTransport::new(Box::new(port), reopen, session.reconnect);
            capture_on(t, device, session, opts, deadline, on_line)
        }
        _ => capture_on(port, device, session, opts, deadline, on_line),
    }
}

/// Re-find the device by USB serial (its port path may change) and reopen it.
fn reopen_by_serial(serial: String, baud: u32) -> Reopen {
    Box::new(move || {
        let port = discovery::enumerate()
            .ok()?
            .into_iter()
            // macOS lists each device twice; the /dev/cu.* node is the one to open.
            .filter(|p| !p.port.starts_with("/dev/tty."))
            .find(|p| {
                p.usb
                    .as_ref()
                    .and_then(|u| u.serial_number.as_deref())
                    .is_some_and(|s| s.eq_ignore_ascii_case(&serial))
            })?;
        let t = SerialTransport::open(&port.port, baud).ok()?;
        Some(Box::new(t) as Box<dyn Transport>)
    })
}

/// Capture from an already-open transport (a serial port, or a replay in tests).
pub fn capture_on<T: Transport>(
    port: T,
    device: ResolvedDevice,
    session: SessionOptions,
    opts: &CaptureOptions,
    deadline: Deadline,
    on_line: &mut dyn FnMut(&LogLine),
) -> Result<SessionOutput, PogoError> {
    let jtag = is_usb_serial_jtag(device.usb_ids);
    let mut warnings = Vec::new();
    let result = match session.record {
        None => {
            let mut t = port;
            run(&mut t, session.reset, jtag, opts, deadline, on_line)?
        }
        Some(sink) => {
            let mut t =
                RecordingTransport::new(port, sink, Some(device.port.clone()), Some(session.baud));
            let result = run(&mut t, session.reset, jtag, opts, deadline, on_line);
            if let Err(e) = t.finish() {
                warnings.push(Warning::new("RECORD_FAILED", e.to_string()));
            }
            result?
        }
    };
    Ok(SessionOutput {
        data: SerialCaptureData {
            device,
            capture: result,
        },
        warnings,
    })
}

/// Compile user-supplied patterns; the error names the flag and the bad pattern.
pub fn compile_patterns(patterns: &[String], flag: &str) -> Result<Vec<regex::Regex>, String> {
    patterns
        .iter()
        .map(|p| {
            regex::Regex::new(p).map_err(|e| {
                let detail = e.to_string();
                let last = detail.lines().last().unwrap_or_default().trim();
                format!("invalid {flag} pattern '{p}': {last}")
            })
        })
        .collect()
}

fn run(
    t: &mut dyn Transport,
    reset: bool,
    jtag: bool,
    opts: &CaptureOptions,
    deadline: Deadline,
    on_line: &mut dyn FnMut(&LogLine),
) -> Result<CaptureResult, PogoError> {
    if reset {
        hard_reset(t, jtag)?;
    }
    capture(t, opts, deadline, on_line)
}
