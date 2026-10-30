//! Real serial ports via the `serialport` crate.

use std::io::{ErrorKind, Read, Write};
use std::time::Duration;

use super::Transport;
use crate::error::PogoError;

pub struct SerialTransport {
    port: Box<dyn serialport::SerialPort>,
    name: String,
}

impl SerialTransport {
    /// Open `name` with DTR and RTS both deasserted. Opening with default line states can
    /// reset boards whose auto-reset circuit watches DTR/RTS (docs/commands.md §7.6).
    pub fn open(name: &str, baud: u32) -> Result<Self, PogoError> {
        let port = serialport::new(name, baud)
            .dtr_on_open(false)
            .timeout(Duration::from_millis(10))
            .open()
            .map_err(|e| open_error(name, &e))?;
        let mut t = SerialTransport {
            port,
            name: name.to_string(),
        };
        t.set_rts(false)?;
        Ok(t)
    }

    fn io(&self, what: &str, e: impl std::fmt::Display) -> PogoError {
        PogoError::DeviceIo(format!("{what} {}: {e}", self.name))
    }
}

fn open_error(name: &str, e: &serialport::Error) -> PogoError {
    match e.kind() {
        serialport::ErrorKind::NoDevice | serialport::ErrorKind::Io(ErrorKind::NotFound) => {
            PogoError::DeviceNotFound {
                selector: name.to_string(),
            }
        }
        serialport::ErrorKind::Io(ErrorKind::PermissionDenied) => PogoError::PermissionDenied {
            port: name.to_string(),
        },
        _ => PogoError::DeviceIo(format!("cannot open {name}: {e}")),
    }
}

impl Transport for SerialTransport {
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, PogoError> {
        // serialport treats a zero timeout as "block forever" on some platforms.
        let timeout = timeout.max(Duration::from_millis(1));
        self.port
            .set_timeout(timeout)
            .map_err(|e| self.io("cannot set timeout on", e))?;
        match self.port.read(buf) {
            Ok(0) => Err(self.io("end of stream on", "device closed")),
            Ok(n) => Ok(n),
            Err(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => Ok(0),
            Err(e) if e.kind() == ErrorKind::Interrupted => Ok(0),
            Err(e) => Err(self.io("read from", e)),
        }
    }

    fn write_all(&mut self, data: &[u8]) -> Result<(), PogoError> {
        self.port
            .write_all(data)
            .and_then(|()| self.port.flush())
            .map_err(|e| self.io("write to", e))
    }

    fn set_dtr(&mut self, asserted: bool) -> Result<(), PogoError> {
        self.port
            .write_data_terminal_ready(asserted)
            .map_err(|e| self.io("cannot set DTR on", e))
    }

    fn set_rts(&mut self, asserted: bool) -> Result<(), PogoError> {
        self.port
            .write_request_to_send(asserted)
            .map_err(|e| self.io("cannot set RTS on", e))
    }

    fn set_baud(&mut self, baud: u32) -> Result<(), PogoError> {
        self.port
            .set_baud_rate(baud)
            .map_err(|e| self.io("cannot set baud on", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    #[test]
    fn open_errors_map_to_stable_codes() {
        let e = |k| serialport::Error::new(k, "x");
        let code = |k| open_error("/dev/cu.x", &e(k)).code();
        assert_eq!(
            code(serialport::ErrorKind::NoDevice),
            ErrorCode::DeviceNotFound
        );
        assert_eq!(
            code(serialport::ErrorKind::Io(ErrorKind::NotFound)),
            ErrorCode::DeviceNotFound
        );
        assert_eq!(
            code(serialport::ErrorKind::Io(ErrorKind::PermissionDenied)),
            ErrorCode::PermissionDenied
        );
        assert_eq!(code(serialport::ErrorKind::Unknown), ErrorCode::DeviceIo);
    }

    #[test]
    fn missing_port_is_device_not_found() {
        let err = SerialTransport::open("/dev/pogo-test-no-such-port", 115_200)
            .err()
            .unwrap();
        assert_eq!(err.code(), ErrorCode::DeviceNotFound, "{err}");
    }
}
