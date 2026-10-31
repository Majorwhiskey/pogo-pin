//! Messages between pogo (host) and the pogo probe firmware (docs/probe-firmware.md §8).
//!
//! `no_std` and allocation-free so the same types build for the host and for the probe
//! (`thumbv6m-none-eabi`, `thumbv8m.main-none-eabihf`). Frames are postcard-serialized,
//! COBS-encoded, and terminated by `0x00`; see [`codec`].

#![no_std]

pub mod codec;

use heapless::{String, Vec};
use serde::{Deserialize, Serialize};

/// Sent in `Response::Hello` so the host knows it reached a pogo probe ("POGO").
pub const MAGIC: u32 = 0x504F_474F;

/// A major mismatch is `PROTOCOL_MISMATCH`; minor versions only add messages.
pub const PROTOCOL_MAJOR: u8 = 1;
pub const PROTOCOL_MINOR: u8 = 0;

/// Largest encoded frame, terminator included.
pub const MAX_FRAME: usize = 512;

/// Pins the firmware will drive: GP2 (EN), GP3 (BOOT), GP8 (PWR_EN), GP9–GP15.
/// Everything else (I2C, USB, flash) is refused with `ProbeError::PinNotAllowed`.
pub const ALLOWED_PINS: u32 = (1 << 2) | (1 << 3) | (1 << 8) | (0x7f << 9);

pub fn pin_allowed(pin: u8) -> bool {
    pin < 32 && ALLOWED_PINS & (1 << pin) != 0
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Frame<T> {
    /// The host's request number; the probe echoes it in every response to that request.
    pub seq: u16,
    pub body: T,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinMode {
    /// Drive low or release (high-Z); never drive high. Used for EN and BOOT.
    OpenDrain,
    PushPull,
    Input,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Low,
    High,
    /// High-impedance: stop driving the pin.
    Release,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Board {
    Rp2040,
    Rp2350,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeError {
    /// The pin is not in [`ALLOWED_PINS`].
    PinNotAllowed,
    /// `GpioSet`/`GpioPulse` on a pin that was not configured first.
    PinNotConfigured,
    /// `Level::High` on an open-drain pin.
    InvalidLevel,
    /// No such I2C bus (0 is the user bus; 1 belongs to the INA226).
    InvalidBus,
    /// The device did not acknowledge its address.
    I2cNack,
    I2cBus,
    /// No INA226 or load switch fitted.
    PowerUnavailable,
    /// A power measurement is already running.
    Busy,
    /// The request is well-formed but not supported by this firmware.
    Unsupported,
    /// The request could not be decoded.
    BadFrame,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Hello {
        protocol_major: u8,
        protocol_minor: u8,
    },
    GetInfo,
    GpioConfigure {
        pin: u8,
        mode: PinMode,
    },
    GpioSet {
        pin: u8,
        level: Level,
    },
    GpioGet {
        pin: u8,
    },
    GpioPulse {
        pin: u8,
        level: Level,
        duration_us: u32,
    },
    I2cScan {
        bus: u8,
        first: u8,
        last: u8,
    },
    I2cRead {
        bus: u8,
        addr: u8,
        reg: Option<u8>,
        len: u8,
    },
    I2cWrite {
        bus: u8,
        addr: u8,
        data: Vec<u8, 64>,
    },
    PowerStart {
        interval_us: u32,
        duration_ms: u32,
    },
    PowerStop,
    PowerSwitch {
        on: bool,
    },
    ReleaseAll,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerSample {
    /// INA226 shunt voltage register, raw.
    pub shunt_raw: i16,
    /// INA226 bus voltage register, raw.
    pub bus_raw: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Response {
    Hello {
        magic: u32,
        protocol_major: u8,
        protocol_minor: u8,
        fw_version: String<16>,
    },
    Info {
        board: Board,
        uptime_ms: u64,
        /// Bit n set: GPn is being driven.
        driven_pins: u32,
    },
    Ok,
    Level(Level),
    /// 7-bit addresses that acknowledged.
    I2cFound(Vec<u8, 112>),
    Bytes(Vec<u8, 64>),
    PowerChunk {
        index: u32,
        samples: Vec<PowerSample, 48>,
    },
    PowerDone {
        total: u32,
        dropped: u32,
    },
    Err(ProbeError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_spells_pogo() {
        assert_eq!(MAGIC.to_be_bytes(), *b"POGO");
    }

    #[test]
    fn allow_list_matches_the_spec() {
        let allowed: Vec<u8, 32> = (0..40).filter(|p| pin_allowed(*p)).collect();
        assert_eq!(allowed.as_slice(), [2, 3, 8, 9, 10, 11, 12, 13, 14, 15]);
    }
}
