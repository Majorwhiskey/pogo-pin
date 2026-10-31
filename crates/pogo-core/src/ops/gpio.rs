//! `gpio get|set|pulse` on the probe's pins, by the names declared in `[pins]`.

use std::time::Duration;

use pogo_protocol::{Level, PinMode, Request, Response, pin_allowed};
use schemars::JsonSchema;
use serde::Serialize;

use super::OpResult;
use super::probe::{Probe, unexpected};
use super::target::{Selection, load_config};
use crate::config::{Config, PinMode as ConfigPinMode};
use crate::envelope::ErrorBody;
use crate::error::{ErrorCode, PogoError};
use crate::probe::REQUEST_TIMEOUT;

/// Longest pulse; anything longer is a `gpio set` followed by another.
pub const MAX_PULSE: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinState {
    Low,
    High,
    Release,
}

impl PinState {
    fn name(self) -> &'static str {
        match self {
            PinState::Low => "low",
            PinState::High => "high",
            PinState::Release => "release",
        }
    }

    fn level(self) -> Level {
        match self {
            PinState::Low => Level::Low,
            PinState::High => Level::High,
            PinState::Release => Level::Release,
        }
    }
}

/// A `[pins]` entry the host is allowed to touch.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pin {
    pub probe_pin: u8,
    pub mode: PinMode,
}

fn refused(reason: impl Into<String>) -> ErrorBody {
    PogoError::GuardrailRefused {
        reason: reason.into(),
    }
    .into()
}

/// Only pins declared in config, and only those on the firmware's allow-list
/// (docs/guardrails.md §11). An undeclared mode is open-drain, which can never drive high.
pub(crate) fn declared(config: &Config, name: &str) -> Result<Pin, ErrorBody> {
    let Some(pin) = config.pins.get(name) else {
        let known: Vec<&str> = config.pins.keys().map(String::as_str).collect();
        let known = if known.is_empty() {
            "none are declared".to_string()
        } else {
            format!("declared: {}", known.join(", "))
        };
        return Err(refused(format!(
            "pin '{name}' is not declared in the config's [pins] table ({known})"
        )));
    };
    let probe_pin = pin.probe_pin.ok_or_else(|| {
        ErrorBody::from(PogoError::Config(format!("pins.{name}: missing probe_pin")))
    })?;
    if !pin_allowed(probe_pin) {
        return Err(refused(format!(
            "pins.{name}: probe pin GP{probe_pin} is not one the probe may drive \
             (GP2, GP3, GP8, GP9-GP15)"
        )));
    }
    let mode = match pin.mode.unwrap_or(ConfigPinMode::OpenDrain) {
        ConfigPinMode::OpenDrain => PinMode::OpenDrain,
        ConfigPinMode::PushPull => PinMode::PushPull,
        ConfigPinMode::Input => PinMode::Input,
    };
    Ok(Pin { probe_pin, mode })
}

/// Refuse states the pin's wiring does not allow.
pub(crate) fn check_drive(name: &str, pin: Pin, level: Level) -> Result<(), ErrorBody> {
    match (pin.mode, level) {
        (PinMode::Input, _) => Err(refused(format!(
            "pin '{name}' is declared as an input; it cannot be driven"
        ))),
        (PinMode::OpenDrain, Level::High) => Err(refused(format!(
            "pin '{name}' is open-drain: use low or release, never high"
        ))),
        _ => Ok(()),
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct GpioGetData {
    pub pin: String,
    pub probe_pin: u8,
    /// `low` or `high`.
    pub level: String,
}

pub fn get(sel: &Selection, name: &str) -> OpResult<GpioGetData> {
    let run = || -> Result<GpioGetData, ErrorBody> {
        let config = load_config(sel)?;
        let pin = declared(&config, name)?;
        let mut probe = Probe::open(sel, config, "gpio.get")?;
        let level = match probe.call(Request::GpioGet { pin: pin.probe_pin })? {
            Response::Level(Level::Low) => "low",
            Response::Level(Level::High) => "high",
            r => return Err(unexpected(&r)),
        };
        Ok(GpioGetData {
            pin: name.to_string(),
            probe_pin: pin.probe_pin,
            level: level.to_string(),
        })
    };
    run().map_or_else(OpResult::fail, OpResult::ok)
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct GpioSetData {
    pub pin: String,
    pub probe_pin: u8,
    /// `low`, `high`, or `release` (high-Z).
    pub state: String,
    pub dry_run: bool,
}

pub fn set(sel: &Selection, name: &str, state: PinState, dry_run: bool) -> OpResult<GpioSetData> {
    let config = match load_config(sel) {
        Ok(c) => c,
        Err(e) => return OpResult::fail(e),
    };
    let pin = match declared(&config, name).and_then(|p| {
        // Releasing is always safe, even for an input.
        if state != PinState::Release {
            check_drive(name, p, state.level())?;
        }
        Ok(p)
    }) {
        Ok(p) => p,
        Err(e) => return OpResult::fail(e),
    };
    let data = GpioSetData {
        pin: name.to_string(),
        probe_pin: pin.probe_pin,
        state: state.name().to_string(),
        dry_run,
    };
    if dry_run {
        return OpResult::ok(data);
    }
    let mut probe = match Probe::open(sel, config, "gpio.set") {
        Ok(p) => p,
        Err(e) => return OpResult::fail(e),
    };
    let result = drive(
        &mut probe,
        pin,
        Request::GpioSet {
            pin: pin.probe_pin,
            level: state.level(),
        },
        REQUEST_TIMEOUT,
    );
    let args =
        serde_json::json!({ "pin": name, "probe_pin": pin.probe_pin, "state": state.name() });
    let warnings = probe.audit("gpio.set", args, &result);
    match result {
        Ok(()) => OpResult::ok(data).with_warnings(warnings),
        Err(e) => OpResult::fail(e).with_warnings(warnings),
    }
}

/// Configure the pin's mode, then send `req`. The firmware keeps a pin's level when it
/// is reconfigured to the mode it already has, so this never glitches a held line.
pub(crate) fn drive(
    probe: &mut Probe,
    pin: Pin,
    req: Request,
    timeout: Duration,
) -> Result<(), ErrorBody> {
    let configure = Request::GpioConfigure {
        pin: pin.probe_pin,
        mode: pin.mode,
    };
    match probe.call(configure)? {
        Response::Ok => {}
        r => return Err(unexpected(&r)),
    }
    match probe.call_with(req, timeout)? {
        Response::Ok => Ok(()),
        r => Err(unexpected(&r)),
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct GpioPulseData {
    pub pin: String,
    pub probe_pin: u8,
    /// The level held during the pulse: `low` or `high`.
    pub level: String,
    pub duration_ms: f64,
    pub dry_run: bool,
}

pub fn pulse(
    sel: &Selection,
    name: &str,
    high: bool,
    duration: Duration,
    dry_run: bool,
) -> OpResult<GpioPulseData> {
    if duration.is_zero() || duration > MAX_PULSE {
        return OpResult::fail(ErrorBody::new(
            ErrorCode::Usage,
            "pulse duration must be between 1µs and 60s",
        ));
    }
    let level = if high { Level::High } else { Level::Low };
    let config = match load_config(sel) {
        Ok(c) => c,
        Err(e) => return OpResult::fail(e),
    };
    let pin = match declared(&config, name).and_then(|p| check_drive(name, p, level).map(|()| p)) {
        Ok(p) => p,
        Err(e) => return OpResult::fail(e),
    };
    let duration_us = u32::try_from(duration.as_micros())
        .unwrap_or(u32::MAX)
        .max(1);
    let data = GpioPulseData {
        pin: name.to_string(),
        probe_pin: pin.probe_pin,
        level: if high { "high" } else { "low" }.to_string(),
        duration_ms: f64::from(duration_us) / 1000.0,
        dry_run,
    };
    if dry_run {
        return OpResult::ok(data);
    }
    let mut probe = match Probe::open(sel, config, "gpio.pulse") {
        Ok(p) => p,
        Err(e) => return OpResult::fail(e),
    };
    let req = Request::GpioPulse {
        pin: pin.probe_pin,
        level,
        duration_us,
    };
    // The probe answers once the pulse is over.
    let result = drive(&mut probe, pin, req, REQUEST_TIMEOUT + duration);
    let args = serde_json::json!({
        "pin": name, "probe_pin": pin.probe_pin, "level": data.level, "duration_us": duration_us,
    });
    let warnings = probe.audit("gpio.pulse", args, &result);
    match result {
        Ok(()) => OpResult::ok(data).with_warnings(warnings),
        Err(e) => OpResult::fail(e).with_warnings(warnings),
    }
}
