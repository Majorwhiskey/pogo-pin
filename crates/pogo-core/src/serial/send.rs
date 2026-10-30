//! `serial send`: write bytes, then optionally capture until a response matches.

use schemars::JsonSchema;
use serde::Serialize;

use super::LogLine;
use super::capture::{CaptureOptions, CaptureResult, capture};
use crate::deadline::Deadline;
use crate::discovery::ResolvedDevice;
use crate::error::PogoError;
use crate::transport::{SerialTransport, Transport};

/// `data` for `pogo serial send`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SerialSendData {
    pub device: ResolvedDevice,
    pub bytes_written: usize,
    /// The capture run for `--expect`; `null` without it.
    pub response: Option<CaptureResult>,
}

/// Open the device's port and send.
pub fn send_device(
    device: ResolvedDevice,
    baud: u32,
    data: &[u8],
    expect: Option<&CaptureOptions>,
    deadline: Deadline,
    on_line: &mut dyn FnMut(&LogLine),
) -> Result<SerialSendData, PogoError> {
    let port = SerialTransport::open(&device.port, baud)?;
    send_on(port, device, data, expect, deadline, on_line)
}

/// Send over an already-open transport. The port is open before writing, so a fast
/// reply is buffered rather than lost.
pub fn send_on<T: Transport>(
    mut port: T,
    device: ResolvedDevice,
    data: &[u8],
    expect: Option<&CaptureOptions>,
    deadline: Deadline,
    on_line: &mut dyn FnMut(&LogLine),
) -> Result<SerialSendData, PogoError> {
    port.write_all(data)?;
    let response = match expect {
        Some(opts) => Some(capture(&mut port, opts, deadline, on_line)?),
        None => None,
    };
    Ok(SerialSendData {
        device,
        bytes_written: data.len(),
        response,
    })
}

/// `"de ad be ef"`, `"deadbeef"`, or `"0xde 0xad"` → bytes.
pub fn parse_hex(s: &str) -> Result<Vec<u8>, String> {
    let digits: String = s
        .split_whitespace()
        .map(|w| w.trim_start_matches("0x").trim_start_matches("0X"))
        .collect();
    if digits.is_empty() || !digits.len().is_multiple_of(2) {
        return Err(format!("'{s}' is not an even number of hex digits"));
    }
    (0..digits.len())
        .step_by(2)
        .map(|i| {
            digits
                .get(i..i + 2)
                .and_then(|p| u8::from_str_radix(p, 16).ok())
                .ok_or_else(|| format!("'{s}' is not valid hex"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use regex::Regex;

    use super::*;
    use crate::config::ParserName;
    use crate::discovery::DeviceKind;
    use crate::serial::DecodeOptions;
    use crate::serial::capture::Outcome;
    use crate::transport::{MockCall, MockTransport, Step};

    fn device() -> ResolvedDevice {
        ResolvedDevice {
            alias: None,
            port: "/dev/mock".into(),
            usb_serial: None,
            kind: DeviceKind::Esp32Target,
            usb_ids: None,
        }
    }

    #[test]
    fn writes_then_captures_the_reply() {
        let t = MockTransport::new([Step::rx("> ping\r\npong\r\n")]);
        let calls = t.calls();
        let opts = CaptureOptions {
            until: vec![Regex::new("^pong$").unwrap()],
            fail_on: vec![],
            settle: Duration::ZERO,
            max_lines: 500,
            decode: DecodeOptions {
                parser: ParserName::Auto,
                keep_ansi: false,
                max_line_len: 1000,
            },
            expect_one_boot: false,
            cancel: None,
        };
        let deadline = Deadline::after(Duration::from_secs(2));
        let out = send_on(t, device(), b"ping\n", Some(&opts), deadline, &mut |_| {}).unwrap();
        assert_eq!(out.bytes_written, 5);
        assert_eq!(
            *calls.lock().unwrap(),
            [MockCall::Write(b"ping\n".to_vec())]
        );
        let r = out.response.unwrap();
        assert_eq!(r.outcome, Outcome::Matched);
        assert_eq!(r.matched.unwrap().line_index, 1);
    }

    #[test]
    fn without_expect_nothing_is_read() {
        let t = MockTransport::new([Step::rx("unread\n")]);
        let deadline = Deadline::after(Duration::from_secs(2));
        let out = send_on(t, device(), b"x", None, deadline, &mut |_| {}).unwrap();
        assert_eq!((out.bytes_written, out.response), (1, None));
    }

    #[test]
    fn hex_forms() {
        assert_eq!(parse_hex("de ad be ef").unwrap(), [0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(parse_hex("DEADbeef").unwrap(), [0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(parse_hex("0x01 0X0a").unwrap(), [0x01, 0x0a]);
        assert!(parse_hex("abc").is_err());
        assert!(parse_hex("zz").is_err());
        assert!(parse_hex("").is_err());
    }
}
