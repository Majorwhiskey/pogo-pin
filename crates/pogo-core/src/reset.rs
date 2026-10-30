//! Resetting a target into run mode through the USB-serial control lines.
//!
//! On ESP32 dev boards RTS drives EN (reset) and DTR drives the boot strap (GPIO0/GPIO9)
//! through the auto-reset circuit; the native USB-Serial-JTAG peripheral emulates the
//! same wiring. The sequence and timings follow esptool's `HardReset`; they have not yet
//! been verified on the bench boards.

use std::time::Duration;

use crate::error::PogoError;
use crate::transport::Transport;

/// Espressif's native USB-Serial-JTAG (ESP32-C3/C6/S3/H2).
pub fn is_usb_serial_jtag(usb_ids: Option<(u16, u16)>) -> bool {
    usb_ids == Some((0x303a, 0x1001))
}

/// Hold EN low, then release it with the boot strap high so the chip runs its app.
pub fn hard_reset(t: &mut dyn Transport, usb_serial_jtag: bool) -> Result<(), PogoError> {
    let (hold, after) = if usb_serial_jtag {
        (Duration::from_millis(200), Duration::from_millis(200))
    } else {
        (Duration::from_millis(100), Duration::ZERO)
    };
    t.set_dtr(false)?;
    t.set_rts(true)?;
    std::thread::sleep(hold);
    t.set_rts(false)?;
    std::thread::sleep(after);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{MockCall, MockTransport};

    #[test]
    fn hard_reset_pulses_rts_with_boot_strap_released() {
        let mut t = MockTransport::new([]);
        let calls = t.calls();
        hard_reset(&mut t, false).unwrap();
        assert_eq!(
            *calls.lock().unwrap(),
            [
                MockCall::Dtr(false),
                MockCall::Rts(true),
                MockCall::Rts(false)
            ]
        );
    }

    #[test]
    fn usb_serial_jtag_detection() {
        assert!(is_usb_serial_jtag(Some((0x303a, 0x1001))));
        assert!(!is_usb_serial_jtag(Some((0x10c4, 0xea60))));
        assert!(!is_usb_serial_jtag(None));
    }
}
