//! Scripted transport for tests: timed byte chunks, delays, and disconnects.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::Transport;
use crate::error::PogoError;

/// One step of a scripted device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Bytes the device sends.
    Rx(Vec<u8>),
    /// The device is silent for this long.
    Delay(Duration),
    /// The device goes away; reads fail with `DEVICE_IO`.
    Disconnect,
}

impl Step {
    pub fn rx(data: impl AsRef<[u8]>) -> Self {
        Step::Rx(data.as_ref().to_vec())
    }
    pub fn delay_ms(ms: u64) -> Self {
        Step::Delay(Duration::from_millis(ms))
    }
}

/// Something the code under test did to the transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockCall {
    Write(Vec<u8>),
    Dtr(bool),
    Rts(bool),
    Baud(u32),
}

/// After the script runs out the device stays connected and silent.
#[derive(Debug)]
pub struct MockTransport {
    steps: VecDeque<Step>,
    delay_until: Option<Instant>,
    calls: Arc<Mutex<Vec<MockCall>>>,
}

impl MockTransport {
    pub fn new(steps: impl IntoIterator<Item = Step>) -> Self {
        MockTransport {
            steps: steps.into_iter().collect(),
            delay_until: None,
            calls: Arc::default(),
        }
    }

    /// A handle that keeps seeing calls after the transport is moved or boxed.
    pub fn calls(&self) -> Arc<Mutex<Vec<MockCall>>> {
        Arc::clone(&self.calls)
    }

    fn log(&self, call: MockCall) {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(call);
    }
}

impl Transport for MockTransport {
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, PogoError> {
        let give_up = Instant::now() + timeout;
        loop {
            match self.steps.front_mut() {
                None => {
                    std::thread::sleep(give_up.saturating_duration_since(Instant::now()));
                    return Ok(0);
                }
                Some(Step::Disconnect) => {
                    return Err(PogoError::DeviceIo("device disconnected (mock)".into()));
                }
                Some(Step::Rx(data)) => {
                    let n = data.len().min(buf.len());
                    buf[..n].copy_from_slice(&data[..n]);
                    data.drain(..n);
                    if data.is_empty() {
                        self.steps.pop_front();
                    }
                    return Ok(n);
                }
                Some(Step::Delay(d)) => {
                    let until = *self.delay_until.get_or_insert_with(|| Instant::now() + *d);
                    if until > give_up {
                        std::thread::sleep(give_up.saturating_duration_since(Instant::now()));
                        return Ok(0);
                    }
                    std::thread::sleep(until.saturating_duration_since(Instant::now()));
                    self.delay_until = None;
                    self.steps.pop_front();
                }
            }
        }
    }

    fn write_all(&mut self, data: &[u8]) -> Result<(), PogoError> {
        self.log(MockCall::Write(data.to_vec()));
        Ok(())
    }
    fn set_dtr(&mut self, asserted: bool) -> Result<(), PogoError> {
        self.log(MockCall::Dtr(asserted));
        Ok(())
    }
    fn set_rts(&mut self, asserted: bool) -> Result<(), PogoError> {
        self.log(MockCall::Rts(asserted));
        Ok(())
    }
    fn set_baud(&mut self, baud: u32) -> Result<(), PogoError> {
        self.log(MockCall::Baud(baud));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(t: &mut MockTransport, ms: u64) -> Result<Vec<u8>, PogoError> {
        let mut buf = [0u8; 4];
        let n = t.read(&mut buf, Duration::from_millis(ms))?;
        Ok(buf[..n].to_vec())
    }

    #[test]
    fn chunks_split_to_buffer_size() {
        let mut t = MockTransport::new([Step::rx("hello"), Step::rx("!")]);
        assert_eq!(read(&mut t, 10).unwrap(), b"hell");
        assert_eq!(read(&mut t, 10).unwrap(), b"o");
        assert_eq!(read(&mut t, 10).unwrap(), b"!");
        assert_eq!(read(&mut t, 5).unwrap(), b"");
    }

    #[test]
    fn delay_longer_than_timeout_returns_empty_then_data() {
        let mut t = MockTransport::new([Step::delay_ms(40), Step::rx("x")]);
        let start = Instant::now();
        assert_eq!(read(&mut t, 10).unwrap(), b"");
        assert_eq!(read(&mut t, 200).unwrap(), b"x");
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(40), "{elapsed:?}");
        assert!(elapsed < Duration::from_millis(200), "{elapsed:?}");
    }

    #[test]
    fn disconnect_is_an_error_and_stays_one() {
        let mut t = MockTransport::new([Step::rx("a"), Step::Disconnect]);
        assert_eq!(read(&mut t, 10).unwrap(), b"a");
        assert!(read(&mut t, 10).is_err());
        assert!(read(&mut t, 10).is_err());
    }

    #[test]
    fn calls_are_logged_through_a_box() {
        let t = MockTransport::new([]);
        let calls = t.calls();
        let mut boxed: Box<dyn Transport> = Box::new(t);
        boxed.set_dtr(false).unwrap();
        boxed.set_rts(true).unwrap();
        boxed.set_baud(115_200).unwrap();
        boxed.write_all(b"ping\n").unwrap();
        assert_eq!(
            *calls.lock().unwrap(),
            [
                MockCall::Dtr(false),
                MockCall::Rts(true),
                MockCall::Baud(115_200),
                MockCall::Write(b"ping\n".to_vec()),
            ]
        );
    }
}
