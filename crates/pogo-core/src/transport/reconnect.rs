//! Survives a device re-enumerating (native USB boards reset their USB connection):
//! after an I/O error, keep trying to reopen the device for a while instead of failing.

use std::time::{Duration, Instant};

use super::Transport;
use crate::error::PogoError;

/// Finds and opens the device again; `None` while it has not reappeared.
pub type Reopen = Box<dyn FnMut() -> Option<Box<dyn Transport>> + Send>;

const RETRY: Duration = Duration::from_millis(100);

pub struct ReconnectingTransport {
    inner: Option<Box<dyn Transport>>,
    reopen: Reopen,
    window: Duration,
    lost: Option<(Instant, String)>,
    reconnects: u32,
}

impl ReconnectingTransport {
    /// `window` is how long the device may be gone before reads fail with `DEVICE_IO`.
    pub fn new(inner: Box<dyn Transport>, reopen: Reopen, window: Duration) -> Self {
        ReconnectingTransport {
            inner: Some(inner),
            reopen,
            window,
            lost: None,
            reconnects: 0,
        }
    }

    pub fn reconnects(&self) -> u32 {
        self.reconnects
    }

    fn gone(&self) -> PogoError {
        let why = self
            .lost
            .as_ref()
            .map_or("device lost", |(_, m)| m.as_str());
        PogoError::DeviceIo(format!(
            "{why}; it did not come back within {}",
            humantime::format_duration(self.window)
        ))
    }

    fn live(&mut self) -> Result<&mut Box<dyn Transport>, PogoError> {
        let err = self.gone();
        self.inner.as_mut().ok_or(err)
    }
}

impl Transport for ReconnectingTransport {
    /// While the device is gone this returns `Ok(0)` (no data yet) and retries, so the
    /// caller's deadline and cancellation keep working.
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, PogoError> {
        if let Some(t) = &mut self.inner {
            return match t.read(buf, timeout) {
                Err(PogoError::DeviceIo(msg)) => {
                    self.inner = None;
                    self.lost = Some((Instant::now(), msg));
                    Ok(0)
                }
                other => other,
            };
        }
        if self
            .lost
            .as_ref()
            .is_some_and(|(at, _)| at.elapsed() > self.window)
        {
            return Err(self.gone());
        }
        match (self.reopen)() {
            Some(t) => {
                self.inner = Some(t);
                self.lost = None;
                self.reconnects += 1;
            }
            None => std::thread::sleep(timeout.min(RETRY)),
        }
        Ok(0)
    }

    fn write_all(&mut self, data: &[u8]) -> Result<(), PogoError> {
        self.live()?.write_all(data)
    }
    fn set_dtr(&mut self, asserted: bool) -> Result<(), PogoError> {
        self.live()?.set_dtr(asserted)
    }
    fn set_rts(&mut self, asserted: bool) -> Result<(), PogoError> {
        self.live()?.set_rts(asserted)
    }
    fn set_baud(&mut self, baud: u32) -> Result<(), PogoError> {
        self.live()?.set_baud(baud)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::transport::{MockTransport, Step};

    fn drain(t: &mut dyn Transport, for_ms: u64) -> Result<Vec<u8>, PogoError> {
        let end = Instant::now() + Duration::from_millis(for_ms);
        let mut out = Vec::new();
        let mut buf = [0u8; 64];
        while Instant::now() < end {
            let n = t.read(&mut buf, Duration::from_millis(10))?;
            out.extend_from_slice(&buf[..n]);
        }
        Ok(out)
    }

    #[test]
    fn reads_continue_after_the_device_comes_back() {
        let attempts = Arc::new(AtomicU32::new(0));
        let seen = Arc::clone(&attempts);
        let reopen: Reopen = Box::new(move || {
            // Not back for the first two attempts.
            (seen.fetch_add(1, Ordering::SeqCst) >= 2)
                .then(|| Box::new(MockTransport::new([Step::rx("after\n")])) as Box<dyn Transport>)
        });
        let first = MockTransport::new([Step::rx("before\n"), Step::Disconnect]);
        let mut t = ReconnectingTransport::new(Box::new(first), reopen, Duration::from_secs(2));
        let got = drain(&mut t, 400).unwrap();
        assert_eq!(got, b"before\nafter\n");
        assert_eq!(t.reconnects(), 1);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn fails_once_the_window_passes() {
        let reopen: Reopen = Box::new(|| None);
        let first = MockTransport::new([Step::Disconnect]);
        let mut t = ReconnectingTransport::new(Box::new(first), reopen, Duration::from_millis(100));
        let start = Instant::now();
        let err = drain(&mut t, 2000).unwrap_err();
        assert!(start.elapsed() >= Duration::from_millis(100));
        assert!(
            err.to_string().contains("did not come back within 100ms"),
            "{err}"
        );
        assert!(t.write_all(b"x").is_err());
    }
}
