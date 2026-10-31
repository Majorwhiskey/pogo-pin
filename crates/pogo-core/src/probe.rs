//! Host side of the probe protocol (docs/probe-firmware.md §8.2, §8.4): handshake, one
//! request at a time, `seq` matching, and a per-request timeout.

use std::time::Duration;

use pogo_protocol::codec::{FrameReader, decode, encode};
use pogo_protocol::{Frame, MAGIC, MAX_FRAME, PROTOCOL_MAJOR, PROTOCOL_MINOR, Request, Response};

use crate::deadline::Deadline;
use crate::error::PogoError;
use crate::transport::Transport;

pub const REQUEST_TIMEOUT: Duration = Duration::from_millis(500);

/// What the probe said in its `Hello`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeHello {
    pub protocol_major: u8,
    pub protocol_minor: u8,
    pub fw_version: String,
}

pub struct ProbeClient<T: Transport> {
    t: T,
    seq: u16,
    reader: FrameReader,
    pub hello: ProbeHello,
}

impl<T: Transport> ProbeClient<T> {
    /// Say hello and check the probe speaks a compatible protocol.
    pub fn connect(t: T) -> Result<Self, PogoError> {
        let mut client = ProbeClient {
            t,
            seq: 0,
            reader: FrameReader::new(),
            hello: ProbeHello {
                protocol_major: 0,
                protocol_minor: 0,
                fw_version: String::new(),
            },
        };
        let hello = Request::Hello {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
        };
        match client.request(hello, REQUEST_TIMEOUT)? {
            Response::Hello {
                magic,
                protocol_major,
                protocol_minor,
                fw_version,
            } => {
                if magic != MAGIC {
                    return Err(PogoError::Protocol(format!(
                        "not a pogo probe (magic {magic:#010x})"
                    )));
                }
                if protocol_major != PROTOCOL_MAJOR {
                    return Err(PogoError::ProtocolMismatch {
                        host_major: PROTOCOL_MAJOR,
                        probe_major: protocol_major,
                    });
                }
                client.hello = ProbeHello {
                    protocol_major,
                    protocol_minor,
                    fw_version: fw_version.as_str().to_string(),
                };
                Ok(client)
            }
            other => Err(PogoError::Protocol(format!(
                "expected Hello, got {other:?}"
            ))),
        }
    }

    /// Send one request and wait for the response carrying its `seq`. Responses to earlier
    /// requests (late arrivals) are skipped.
    pub fn request(&mut self, body: Request, timeout: Duration) -> Result<Response, PogoError> {
        self.seq = self.seq.wrapping_add(1);
        let seq = self.seq;
        let mut buf = [0u8; MAX_FRAME];
        let bytes = encode(&Frame { seq, body }, &mut buf)
            .map_err(|e| PogoError::Protocol(format!("cannot encode request: {e:?}")))?;
        self.t.write_all(bytes)?;

        let deadline = Deadline::after(timeout);
        let mut chunk = [0u8; 64];
        loop {
            let Some(left) = deadline.remaining() else {
                return Err(PogoError::Protocol(format!(
                    "no response to request {seq} within {}",
                    humantime::format_duration(timeout)
                )));
            };
            let n = self.t.read(&mut chunk, left)?;
            for &byte in &chunk[..n] {
                let Some(frame) = self.reader.push(byte) else {
                    continue;
                };
                let frame = frame
                    .and_then(decode::<Response>)
                    .map_err(|e| PogoError::Protocol(format!("bad frame from probe: {e:?}")))?;
                if frame.seq == seq {
                    return Ok(frame.body);
                }
            }
        }
    }

    pub fn into_transport(self) -> T {
        self.t
    }
}

#[cfg(test)]
mod tests {
    use heapless::String as HString;

    use super::*;
    use crate::error::ErrorCode;
    use crate::transport::{MockCall, MockTransport, Step};

    fn wire(seq: u16, body: Response) -> Vec<u8> {
        let mut buf = [0u8; MAX_FRAME];
        encode(&Frame { seq, body }, &mut buf).unwrap().to_vec()
    }

    fn hello(magic: u32, major: u8) -> Response {
        Response::Hello {
            magic,
            protocol_major: major,
            protocol_minor: 3,
            fw_version: HString::try_from("0.1.0").unwrap(),
        }
    }

    #[test]
    fn handshake_then_request() {
        let mut steps = vec![Step::Rx(wire(1, hello(MAGIC, PROTOCOL_MAJOR)))];
        steps.push(Step::Rx(wire(2, Response::Ok)));
        let t = MockTransport::new(steps);
        let calls = t.calls();
        let mut c = ProbeClient::connect(t).unwrap();
        assert_eq!(c.hello.fw_version, "0.1.0");
        assert_eq!(c.hello.protocol_minor, 3);
        let r = c.request(Request::ReleaseAll, REQUEST_TIMEOUT).unwrap();
        assert_eq!(r, Response::Ok);

        // The first write is a Hello with seq 1 that the probe could decode.
        let calls = calls.lock().unwrap();
        let MockCall::Write(first) = &calls[0] else {
            panic!("{calls:?}")
        };
        let mut first = first.clone();
        let sent: Frame<Request> = decode(&mut first).unwrap();
        assert_eq!(sent.seq, 1);
        assert!(matches!(
            sent.body,
            Request::Hello {
                protocol_major: PROTOCOL_MAJOR,
                ..
            }
        ));
    }

    #[test]
    fn wrong_magic_and_major_are_rejected() {
        let t = MockTransport::new([Step::Rx(wire(1, hello(0x4245_4E43, PROTOCOL_MAJOR)))]);
        let err = ProbeClient::connect(t).err().unwrap();
        assert_eq!(err.code(), ErrorCode::Protocol);
        assert!(err.to_string().contains("not a pogo probe"), "{err}");

        let t = MockTransport::new([Step::Rx(wire(1, hello(MAGIC, PROTOCOL_MAJOR + 1)))]);
        let err = ProbeClient::connect(t).err().unwrap();
        assert_eq!(err.code(), ErrorCode::ProtocolMismatch);
        assert_eq!(err.exit_code(), 6);
    }

    #[test]
    fn stale_responses_are_skipped_and_silence_times_out() {
        let steps = vec![
            Step::Rx(wire(1, hello(MAGIC, PROTOCOL_MAJOR))),
            Step::Rx(wire(1, Response::Ok)),
            Step::Rx(wire(2, Response::Level(pogo_protocol::Level::High))),
        ];
        let mut c = ProbeClient::connect(MockTransport::new(steps)).unwrap();
        let r = c
            .request(Request::GpioGet { pin: 2 }, REQUEST_TIMEOUT)
            .unwrap();
        assert_eq!(r, Response::Level(pogo_protocol::Level::High));

        let err = c
            .request(Request::GetInfo, Duration::from_millis(30))
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::Protocol);
        assert!(
            err.to_string().contains("no response to request 3"),
            "{err}"
        );
    }

    #[test]
    fn garbage_from_the_probe_is_a_protocol_error() {
        let t = MockTransport::new([Step::rx([0x05, 0xff, 0xff, 0x00])]);
        let err = ProbeClient::connect(t).err().unwrap();
        assert_eq!(err.code(), ErrorCode::Protocol);
    }
}
