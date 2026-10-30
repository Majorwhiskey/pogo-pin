//! Replays a recorded session (see `record.rs` for the format).
//!
//! Chunks are delivered as fast as they are read; recorded timestamps are not honored,
//! so replays run instantly. After the last event the device is connected but silent,
//! unless the recording ends in `disconnect`. Writes and control-line changes are accepted
//! and ignored.

use std::collections::VecDeque;
use std::path::Path;
use std::time::Duration;

use super::Transport;
use super::record::{RECORD_FORMAT, RecordEvent};
use crate::error::PogoError;

#[derive(Debug)]
pub struct ReplayTransport {
    chunks: VecDeque<Vec<u8>>,
    ends_in_disconnect: bool,
}

impl ReplayTransport {
    pub fn from_file(path: &Path) -> Result<Self, PogoError> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            PogoError::Internal(format!("cannot read recording {}: {e}", path.display()))
        })?;
        Self::from_jsonl(&text).map_err(|e| PogoError::Internal(format!("{}: {e}", path.display())))
    }

    pub fn from_jsonl(text: &str) -> Result<Self, String> {
        let mut lines = text
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty());
        let parse = |(i, line): (usize, &str)| {
            serde_json::from_str::<RecordEvent>(line).map_err(|e| format!("line {}: {e}", i + 1))
        };
        match lines.next().map(parse).transpose()? {
            Some(RecordEvent::Header { format, .. }) if format == RECORD_FORMAT => {}
            Some(RecordEvent::Header { format, .. }) => {
                return Err(format!(
                    "recording format {format} is not supported (expected {RECORD_FORMAT})"
                ));
            }
            _ => return Err("line 1: a recording must start with a header event".into()),
        }
        let mut chunks = VecDeque::new();
        let mut ends_in_disconnect = false;
        for (i, line) in lines {
            if ends_in_disconnect {
                return Err(format!("line {}: event after disconnect", i + 1));
            }
            match parse((i, line))? {
                RecordEvent::Rx { data, .. } => chunks.push_back(
                    data.to_bytes()
                        .map_err(|e| format!("line {}: {e}", i + 1))?,
                ),
                RecordEvent::Disconnect { .. } => ends_in_disconnect = true,
                RecordEvent::Header { .. } => {
                    return Err(format!("line {}: duplicate header", i + 1));
                }
                _ => {}
            }
        }
        Ok(ReplayTransport {
            chunks,
            ends_in_disconnect,
        })
    }
}

impl Transport for ReplayTransport {
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, PogoError> {
        let Some(chunk) = self.chunks.front_mut() else {
            if self.ends_in_disconnect {
                return Err(PogoError::DeviceIo("device disconnected (replay)".into()));
            }
            std::thread::sleep(timeout);
            return Ok(0);
        };
        let n = chunk.len().min(buf.len());
        buf[..n].copy_from_slice(&chunk[..n]);
        chunk.drain(..n);
        if chunk.is_empty() {
            self.chunks.pop_front();
        }
        Ok(n)
    }

    fn write_all(&mut self, _data: &[u8]) -> Result<(), PogoError> {
        Ok(())
    }
    fn set_dtr(&mut self, _asserted: bool) -> Result<(), PogoError> {
        Ok(())
    }
    fn set_rts(&mut self, _asserted: bool) -> Result<(), PogoError> {
        Ok(())
    }
    fn set_baud(&mut self, _baud: u32) -> Result<(), PogoError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{MockTransport, Payload, RecordingTransport, Step};

    fn drain(t: &mut dyn Transport) -> (Vec<u8>, bool) {
        let mut out = Vec::new();
        let mut buf = [0u8; 3];
        loop {
            match t.read(&mut buf, Duration::from_millis(5)) {
                Ok(0) => return (out, false),
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(_) => return (out, true),
            }
        }
    }

    #[test]
    fn record_then_replay_round_trips() {
        let script = [
            Step::rx("I (31) boot: ESP-IDF\r\n"),
            Step::rx([0xff, 0x00, b'\x1b']),
            Step::rx("READY\n"),
            Step::Disconnect,
        ];
        let mut rec = RecordingTransport::new(
            MockTransport::new(script),
            Vec::new(),
            Some("/dev/cu.usbmodem1".into()),
            Some(115_200),
        );
        rec.set_dtr(false).unwrap();
        rec.write_all(b"ping\n").unwrap();
        let (live, disconnected) = drain(&mut rec);
        assert!(disconnected);
        let jsonl = String::from_utf8(rec.finish().unwrap()).unwrap();

        let first: serde_json::Value = serde_json::from_str(jsonl.lines().next().unwrap()).unwrap();
        assert_eq!(first["type"], "header");
        assert_eq!(first["format"], 1);
        assert!(jsonl.contains(r#""type":"tx""#) && jsonl.contains(r#""hex":"ff001b""#));
        let last: serde_json::Value = serde_json::from_str(jsonl.lines().last().unwrap()).unwrap();
        assert_eq!(last["type"], "disconnect");

        let mut replay = ReplayTransport::from_jsonl(&jsonl).unwrap();
        assert_eq!(drain(&mut replay), (live, true));
    }

    #[test]
    fn recording_without_disconnect_goes_idle() {
        let text = concat!(
            r#"{"type":"header","format":1,"pogo_version":"0.1.0"}"#,
            "\n",
            r#"{"type":"rx","t_ms":0,"text":"hi\n"}"#,
            "\n",
        );
        let mut t = ReplayTransport::from_jsonl(text).unwrap();
        assert_eq!(drain(&mut t), (b"hi\n".to_vec(), false));
    }

    #[test]
    fn malformed_recordings_are_rejected() {
        let header = r#"{"type":"header","format":1,"pogo_version":"0.1.0"}"#;
        for (body, want) in [
            (r#"{"type":"rx","t_ms":0,"text":"x"}"#.to_string(), "header"),
            (
                r#"{"type":"header","format":2,"pogo_version":"9"}"#.into(),
                "format 2",
            ),
            (
                format!("{header}\n{{\"type\":\"rx\",\"t_ms\":0}}"),
                "exactly one",
            ),
            (
                format!("{header}\n{{\"type\":\"rx\",\"t_ms\":0,\"hex\":\"f\"}}"),
                "odd-length",
            ),
            (
                format!("{header}\n{{\"type\":\"beep\",\"t_ms\":0}}"),
                "line 2",
            ),
            (
                format!(
                    "{header}\n{{\"type\":\"disconnect\",\"t_ms\":1}}\n{{\"type\":\"rx\",\"t_ms\":2,\"text\":\"x\"}}"
                ),
                "after disconnect",
            ),
        ] {
            let err = ReplayTransport::from_jsonl(&body).unwrap_err();
            assert!(err.contains(want), "{want}: {err}");
        }
    }

    #[test]
    fn payload_hex_and_text() {
        assert_eq!(Payload::from_bytes(b"ok").text.as_deref(), Some("ok"));
        // 0xff never appears in UTF-8, so this must fall back to hex.
        let p = Payload::from_bytes(&[0xff, 0x00, 0xab]);
        assert_eq!(p.text, None);
        assert_eq!(p.hex.as_deref(), Some("ff00ab"));
        assert_eq!(p.to_bytes().unwrap(), [0xff, 0x00, 0xab]);
        // A multi-byte UTF-8 character stays readable text.
        assert_eq!(
            Payload::from_bytes(&[0xde, 0xad]).text.as_deref(),
            Some("\u{7ad}")
        );
    }
}
