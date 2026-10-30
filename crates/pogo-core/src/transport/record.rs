//! Session recording format (`--record`, `tests/fixtures/*.jsonl`) and the recorder.
//!
//! One JSON object per line. The first line is a `header`; then events in time order:
//! `{"type":"rx","t_ms":12,"text":"I (31) boot: ...\r\n"}`. Byte chunks carry `text`
//! when they are valid UTF-8, otherwise `hex`, so fixtures stay readable and editable.

use std::io::Write;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::Transport;
use crate::error::PogoError;

pub const RECORD_FORMAT: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecordEvent {
    Header {
        format: u32,
        pogo_version: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        port: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        baud: Option<u32>,
    },
    Rx {
        t_ms: u64,
        #[serde(flatten)]
        data: Payload,
    },
    Tx {
        t_ms: u64,
        #[serde(flatten)]
        data: Payload,
    },
    Dtr {
        t_ms: u64,
        asserted: bool,
    },
    Rts {
        t_ms: u64,
        asserted: bool,
    },
    Baud {
        t_ms: u64,
        baud: u32,
    },
    /// The device went away.
    Disconnect {
        t_ms: u64,
    },
}

/// A byte chunk as `text` (valid UTF-8) or lowercase `hex`. Exactly one is set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hex: Option<String>,
}

impl Payload {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        match std::str::from_utf8(bytes) {
            Ok(s) => Payload {
                text: Some(s.to_string()),
                hex: None,
            },
            Err(_) => Payload {
                text: None,
                hex: Some(bytes.iter().map(|b| format!("{b:02x}")).collect()),
            },
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        match (&self.text, &self.hex) {
            (Some(t), None) => Ok(t.as_bytes().to_vec()),
            (None, Some(h)) => {
                if !h.len().is_multiple_of(2) {
                    return Err(format!("odd-length hex '{h}'"));
                }
                (0..h.len())
                    .step_by(2)
                    .map(|i| {
                        h.get(i..i + 2)
                            .and_then(|p| u8::from_str_radix(p, 16).ok())
                            .ok_or_else(|| format!("invalid hex '{h}'"))
                    })
                    .collect()
            }
            _ => Err("a chunk needs exactly one of `text` or `hex`".into()),
        }
    }
}

/// Wraps a transport and writes every event to `sink` as it happens.
pub struct RecordingTransport<T, W> {
    inner: T,
    sink: W,
    start: Instant,
    error: Option<std::io::Error>,
}

impl<T: Transport, W: Write + Send> RecordingTransport<T, W> {
    pub fn new(inner: T, sink: W, port: Option<String>, baud: Option<u32>) -> Self {
        let mut r = RecordingTransport {
            inner,
            sink,
            start: Instant::now(),
            error: None,
        };
        r.emit(RecordEvent::Header {
            format: RECORD_FORMAT,
            pogo_version: crate::envelope::POGO_VERSION.to_string(),
            port,
            baud,
        });
        r
    }

    /// Flush and return the sink, or the first error writing the recording.
    pub fn finish(mut self) -> Result<W, PogoError> {
        if self.error.is_none() {
            self.error = self.sink.flush().err();
        }
        match self.error {
            Some(e) => Err(PogoError::Internal(format!("cannot write recording: {e}"))),
            None => Ok(self.sink),
        }
    }

    fn t_ms(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Recording problems must not abort the session; the first one is kept for `finish`.
    fn emit(&mut self, event: RecordEvent) {
        if self.error.is_some() {
            return;
        }
        let line = serde_json::to_string(&event).map_err(std::io::Error::other);
        let result = line.and_then(|l| writeln!(self.sink, "{l}"));
        self.error = result.err();
    }
}

impl<T: Transport, W: Write + Send> Transport for RecordingTransport<T, W> {
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, PogoError> {
        let result = self.inner.read(buf, timeout);
        let t_ms = self.t_ms();
        match &result {
            Ok(0) => {}
            Ok(n) => self.emit(RecordEvent::Rx {
                t_ms,
                data: Payload::from_bytes(&buf[..*n]),
            }),
            Err(PogoError::DeviceIo(_)) => self.emit(RecordEvent::Disconnect { t_ms }),
            Err(_) => {}
        }
        result
    }

    fn write_all(&mut self, data: &[u8]) -> Result<(), PogoError> {
        let t_ms = self.t_ms();
        self.emit(RecordEvent::Tx {
            t_ms,
            data: Payload::from_bytes(data),
        });
        self.inner.write_all(data)
    }

    fn set_dtr(&mut self, asserted: bool) -> Result<(), PogoError> {
        let t_ms = self.t_ms();
        self.emit(RecordEvent::Dtr { t_ms, asserted });
        self.inner.set_dtr(asserted)
    }

    fn set_rts(&mut self, asserted: bool) -> Result<(), PogoError> {
        let t_ms = self.t_ms();
        self.emit(RecordEvent::Rts { t_ms, asserted });
        self.inner.set_rts(asserted)
    }

    fn set_baud(&mut self, baud: u32) -> Result<(), PogoError> {
        let t_ms = self.t_ms();
        self.emit(RecordEvent::Baud { t_ms, baud });
        self.inner.set_baud(baud)
    }
}
