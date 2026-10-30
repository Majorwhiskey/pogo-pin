//! Byte transports. All device I/O goes through [`Transport`] so everything above it can be
//! tested with [`MockTransport`] or [`ReplayTransport`] instead of hardware.

mod mock;
mod reconnect;
mod record;
mod replay;
mod serial;

use std::time::Duration;

use crate::error::PogoError;

pub use mock::{MockCall, MockTransport, Step};
pub use reconnect::{ReconnectingTransport, Reopen};
pub use record::{Payload, RECORD_FORMAT, RecordEvent, RecordingTransport};
pub use replay::ReplayTransport;
pub use serial::SerialTransport;

pub trait Transport: Send {
    /// Read available bytes, waiting at most `timeout`. `Ok(0)` means no data yet.
    /// A device that has gone away is an error, never `Ok(0)`.
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, PogoError>;
    fn write_all(&mut self, data: &[u8]) -> Result<(), PogoError>;
    fn set_dtr(&mut self, asserted: bool) -> Result<(), PogoError>;
    fn set_rts(&mut self, asserted: bool) -> Result<(), PogoError>;
    fn set_baud(&mut self, baud: u32) -> Result<(), PogoError>;
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, PogoError> {
        (**self).read(buf, timeout)
    }
    fn write_all(&mut self, data: &[u8]) -> Result<(), PogoError> {
        (**self).write_all(data)
    }
    fn set_dtr(&mut self, asserted: bool) -> Result<(), PogoError> {
        (**self).set_dtr(asserted)
    }
    fn set_rts(&mut self, asserted: bool) -> Result<(), PogoError> {
        (**self).set_rts(asserted)
    }
    fn set_baud(&mut self, baud: u32) -> Result<(), PogoError> {
        (**self).set_baud(baud)
    }
}
