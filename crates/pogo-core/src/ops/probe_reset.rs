//! Resetting the target through the probe's `target_en` and `target_boot` pins, with
//! esptool's classic reset timings (the same ones the DTR/RTS reset uses).

use std::time::Duration;

use pogo_protocol::{Level, Request};

use super::gpio::{Pin, check_drive, declared, drive};
use super::probe::Probe;
use super::reset::ResetMode;
use crate::config::Config;
use crate::envelope::ErrorBody;
use crate::error::PogoError;
use crate::probe::REQUEST_TIMEOUT;

pub const EN_PIN: &str = "target_en";
pub const BOOT_PIN: &str = "target_boot";
/// esptool holds EN low this long, then waits this long before releasing BOOT.
const EN_HOLD: Duration = Duration::from_millis(100);
const BOOT_SETTLE: Duration = Duration::from_millis(50);

/// Whether the config declares the pins a probe reset in `mode` needs.
pub fn configured(config: &Config, mode: ResetMode) -> bool {
    config.pins.contains_key(EN_PIN)
        && (mode == ResetMode::Run || config.pins.contains_key(BOOT_PIN))
}

/// The pins for `mode`, checked against the GPIO guardrails before the probe is touched.
pub(crate) fn pins(config: &Config, mode: ResetMode) -> Result<(Pin, Option<Pin>), ErrorBody> {
    if !configured(config, mode) {
        let need = match mode {
            ResetMode::Run => "[pins.target_en]",
            ResetMode::Bootloader => "[pins.target_en] and [pins.target_boot]",
        };
        return Err(PogoError::Config(format!(
            "a probe reset needs {need} in the config (docs/configuration.md)"
        ))
        .into());
    }
    let en = declared(config, EN_PIN)?;
    check_drive(EN_PIN, en, Level::Low)?;
    let boot = match mode {
        ResetMode::Run => config
            .pins
            .get(BOOT_PIN)
            .map(|_| declared(config, BOOT_PIN))
            .transpose()?,
        ResetMode::Bootloader => Some(declared(config, BOOT_PIN)?),
    };
    if let Some(b) = boot {
        check_drive(BOOT_PIN, b, Level::Low)?;
    }
    Ok((en, boot))
}

fn set(probe: &mut Probe, pin: Pin, level: Level) -> Result<(), ErrorBody> {
    let req = Request::GpioSet {
        pin: pin.probe_pin,
        level,
    };
    drive(probe, pin, req, REQUEST_TIMEOUT)
}

/// Pulse EN low with BOOT released (run) or held low (bootloader). BOOT is released
/// again even if the pulse fails, so the next reset boots the app.
pub(crate) fn reset(
    probe: &mut Probe,
    en: Pin,
    boot: Option<Pin>,
    mode: ResetMode,
) -> Result<(), ErrorBody> {
    let hold_boot = mode == ResetMode::Bootloader;
    if let Some(b) = boot {
        set(
            probe,
            b,
            if hold_boot {
                Level::Low
            } else {
                Level::Release
            },
        )?;
    }
    let pulse = Request::GpioPulse {
        pin: en.probe_pin,
        level: Level::Low,
        duration_us: u32::try_from(EN_HOLD.as_micros()).unwrap_or(u32::MAX),
    };
    let pulsed = drive(probe, en, pulse, REQUEST_TIMEOUT + EN_HOLD);
    if let (true, Some(b)) = (hold_boot, boot) {
        std::thread::sleep(BOOT_SETTLE);
        set(probe, b, Level::Release)?;
    }
    pulsed
}

#[cfg(test)]
mod tests {
    use pogo_protocol::codec::decode;
    use pogo_protocol::{PinMode, Response};

    use super::super::probe::tests::{script, setup};
    use super::super::reset::{ResetVia, reset};
    use super::*;
    use crate::transport::MockCall;

    const PINS: &str = "[pins.target_boot]\nprobe_pin = 3\nmode = \"open-drain\"\n";

    #[test]
    fn bootloader_holds_boot_low_across_the_en_pulse() {
        let sel = setup(PINS);
        let calls = script(&vec![Response::Ok; 6]);
        let r = reset(&sel, ResetMode::Bootloader, ResetVia::Probe, false);
        assert!(r.error.is_none(), "{:?}", r.error);
        assert_eq!(r.data.unwrap().via, "probe");
        let sent: Vec<Request> = calls
            .lock()
            .unwrap()
            .iter()
            .skip(1)
            .filter_map(|c| match c {
                MockCall::Write(b) => Some(decode::<Request>(&mut b.clone()).unwrap().body),
                _ => None,
            })
            .collect();
        let od = PinMode::OpenDrain;
        assert_eq!(
            sent,
            [
                Request::GpioConfigure { pin: 3, mode: od },
                Request::GpioSet {
                    pin: 3,
                    level: Level::Low
                },
                Request::GpioConfigure { pin: 2, mode: od },
                Request::GpioPulse {
                    pin: 2,
                    level: Level::Low,
                    duration_us: 100_000
                },
                Request::GpioConfigure { pin: 3, mode: od },
                Request::GpioSet {
                    pin: 3,
                    level: Level::Release
                },
            ]
        );
    }
}
