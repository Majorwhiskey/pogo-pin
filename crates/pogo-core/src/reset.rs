//! Resetting a target into run mode through the USB-serial control lines.
//!
//! On ESP32 dev boards RTS drives EN (reset) and DTR drives the boot strap (GPIO0/GPIO9)
//! through the auto-reset circuit; the native USB-Serial-JTAG peripheral emulates the
//! same wiring. Sequences and timings follow esptool's `HardReset`, `ClassicReset`, and
//! `USBJTAGSerialReset`. `hard_reset` is verified on an ESP32-WROOM-32 devkit (CH340).

use std::time::Duration;

use schemars::JsonSchema;
use serde::Serialize;

use crate::discovery::ResolvedDevice;
use crate::error::PogoError;
use crate::transport::Transport;

/// `data` for `pogo reset`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ResetData {
    pub device: ResolvedDevice,
    /// `run` or `bootloader`.
    pub mode: String,
    /// `dtr-rts` (the probe is not supported yet).
    pub via: String,
    /// The device dropped off USB and came back (native USB boards).
    pub reconnected: bool,
    /// Set with `--dry-run`: nothing was done.
    pub dry_run: bool,
}

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

/// Reset with the boot strap held low so the chip stays in the ROM download mode.
pub fn enter_bootloader(t: &mut dyn Transport, usb_serial_jtag: bool) -> Result<(), PogoError> {
    let ms = |n| std::thread::sleep(Duration::from_millis(n));
    if usb_serial_jtag {
        t.set_rts(false)?;
        t.set_dtr(false)?;
        ms(100);
        t.set_dtr(true)?;
        t.set_rts(false)?;
        ms(100);
        t.set_rts(true)?;
        t.set_dtr(false)?;
        t.set_rts(true)?;
        ms(100);
        t.set_dtr(false)?;
        t.set_rts(false)?;
    } else {
        t.set_dtr(false)?;
        t.set_rts(true)?;
        ms(100);
        t.set_dtr(true)?;
        t.set_rts(false)?;
        ms(50);
        t.set_dtr(false)?;
    }
    Ok(())
}

/// Native USB boards drop off the bus when the chip resets. Watch for the device (by
/// USB serial) to vanish within `vanish`, then to return within `back`. Returns whether it
/// re-enumerated; a device that vanishes and stays gone is `DEVICE_IO`.
pub fn wait_for_reenumeration(
    usb_serial: &str,
    vanish: Duration,
    back: Duration,
) -> Result<bool, PogoError> {
    let present = || {
        crate::discovery::enumerate().map(|ports| {
            ports.iter().any(|p| {
                p.usb
                    .as_ref()
                    .and_then(|u| u.serial_number.as_deref())
                    .is_some_and(|s| s.eq_ignore_ascii_case(usb_serial))
            })
        })
    };
    let poll = Duration::from_millis(50);
    let gone_by = crate::deadline::Deadline::after(vanish);
    while present()? {
        if gone_by.is_expired() {
            return Ok(false);
        }
        std::thread::sleep(poll);
    }
    let back_by = crate::deadline::Deadline::after(back);
    while !present()? {
        if back_by.is_expired() {
            return Err(PogoError::DeviceIo(format!(
                "device {usb_serial} did not come back within {} after reset",
                humantime::format_duration(back)
            )));
        }
        std::thread::sleep(poll);
    }
    Ok(true)
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
    fn bootloader_entry_holds_the_strap_low_while_releasing_en() {
        let mut t = MockTransport::new([]);
        let calls = t.calls();
        enter_bootloader(&mut t, false).unwrap();
        assert_eq!(
            *calls.lock().unwrap(),
            [
                MockCall::Dtr(false),
                MockCall::Rts(true),
                MockCall::Dtr(true),
                MockCall::Rts(false),
                MockCall::Dtr(false)
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
