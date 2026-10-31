//! Framing (docs/probe-firmware.md §8.2): postcard, then COBS, then a `0x00` terminator.
//! Allocation-free, so the probe uses the same code as the host.

use serde::{Deserialize, Serialize};

use crate::{Frame, MAX_FRAME};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    /// The frame would not fit in `MAX_FRAME` bytes.
    TooLarge,
    /// The bytes are not a valid COBS frame of the expected message type.
    Malformed,
}

/// Encode `frame` into `buf`; the returned slice ends with the `0x00` terminator.
pub fn encode<'a, T: Serialize>(
    frame: &Frame<T>,
    buf: &'a mut [u8; MAX_FRAME],
) -> Result<&'a [u8], CodecError> {
    match postcard::to_slice_cobs(frame, buf) {
        Ok(bytes) => Ok(bytes),
        Err(postcard::Error::SerializeBufferFull) => Err(CodecError::TooLarge),
        Err(_) => Err(CodecError::Malformed),
    }
}

/// Decode one frame (COBS bytes, terminator optional) in place.
pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a mut [u8]) -> Result<Frame<T>, CodecError> {
    postcard::from_bytes_cobs(bytes).map_err(|_| CodecError::Malformed)
}

/// Splits a byte stream into frames at each `0x00`.
pub struct FrameReader {
    buf: [u8; MAX_FRAME],
    len: usize,
    overflowed: bool,
}

impl Default for FrameReader {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameReader {
    pub const fn new() -> Self {
        FrameReader {
            buf: [0; MAX_FRAME],
            len: 0,
            overflowed: false,
        }
    }

    /// Feed one byte. At a terminator this yields the frame's COBS bytes (ready for
    /// [`decode`]), or `TooLarge` if the frame overflowed and was dropped. Empty frames
    /// (repeated terminators) are skipped. After an overflow the reader resynchronizes at
    /// the next terminator.
    pub fn push(&mut self, byte: u8) -> Option<Result<&mut [u8], CodecError>> {
        if byte != 0 {
            if self.len < MAX_FRAME - 1 {
                self.buf[self.len] = byte;
                self.len += 1;
            } else {
                self.overflowed = true;
            }
            return None;
        }
        let len = core::mem::take(&mut self.len);
        if core::mem::take(&mut self.overflowed) {
            return Some(Err(CodecError::TooLarge));
        }
        (len > 0).then(|| Ok(&mut self.buf[..len]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Request, Response};

    #[test]
    fn encoded_frames_end_in_one_terminator() {
        let mut buf = [0u8; MAX_FRAME];
        let frame = Frame {
            seq: 7,
            body: Request::GetInfo,
        };
        let bytes = encode(&frame, &mut buf).unwrap();
        assert_eq!(bytes.last(), Some(&0));
        assert!(!bytes[..bytes.len() - 1].contains(&0));
    }

    #[test]
    fn reader_skips_empty_frames_and_reports_overflow_once() {
        let mut r = FrameReader::new();
        assert!(r.push(0).is_none());
        for _ in 0..MAX_FRAME + 10 {
            assert!(r.push(1).is_none());
        }
        assert_eq!(
            r.push(0).map(|f| f.map(|_| ())),
            Some(Err(CodecError::TooLarge))
        );

        let mut buf = [0u8; MAX_FRAME];
        let ok = Frame {
            seq: 1,
            body: Response::Ok,
        };
        let bytes = encode(&ok, &mut buf).unwrap();
        let mut got = None;
        for b in bytes {
            if let Some(f) = r.push(*b) {
                got = Some(decode::<Response>(f.unwrap()).unwrap());
            }
        }
        assert_eq!(got, Some(ok));
    }

    #[test]
    fn garbage_is_malformed_not_a_panic() {
        let mut junk = [0xff, 0x13, 0x02, 0x99];
        assert_eq!(decode::<Request>(&mut junk), Err(CodecError::Malformed));
    }
}
