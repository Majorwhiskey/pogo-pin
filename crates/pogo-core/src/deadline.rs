//! Deadlines for blocking operations: every core call that can block takes one.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline(Instant);

impl Deadline {
    pub fn after(d: Duration) -> Self {
        // A duration too large to add saturates to roughly "never" instead of panicking.
        let now = Instant::now();
        Deadline(
            now.checked_add(d)
                .unwrap_or_else(|| now + Duration::from_secs(100 * 365 * 24 * 3600)),
        )
    }

    /// Time left, or `None` once expired.
    pub fn remaining(&self) -> Option<Duration> {
        self.0
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
    }

    pub fn is_expired(&self) -> bool {
        self.remaining().is_none()
    }

    /// The earlier of this deadline and one `d` from now.
    pub fn min_after(&self, d: Duration) -> Self {
        Deadline(self.0.min(Deadline::after(d).0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expires() {
        let d = Deadline::after(Duration::from_millis(20));
        assert!(d.remaining().unwrap() <= Duration::from_millis(20));
        assert!(!d.is_expired());
        std::thread::sleep(Duration::from_millis(30));
        assert!(d.is_expired());
        assert_eq!(d.remaining(), None);
    }

    #[test]
    fn zero_is_already_expired_and_huge_does_not_panic() {
        assert!(Deadline::after(Duration::ZERO).is_expired());
        assert!(!Deadline::after(Duration::MAX).is_expired());
    }

    #[test]
    fn min_after_picks_the_earlier() {
        let long = Deadline::after(Duration::from_secs(60));
        let short = long.min_after(Duration::from_millis(10));
        assert!(short.remaining().unwrap() <= Duration::from_millis(10));
        let same = short.min_after(Duration::from_secs(60));
        assert_eq!(same, short);
    }
}
