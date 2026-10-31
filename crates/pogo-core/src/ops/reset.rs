//! `reset` as an op.

use std::time::Duration;

use super::OpResult;
use super::probe::Probe;
use super::probe_reset;
use super::target::{Selection, Target, acquire, acquire_probe};
use crate::audit::{self, AuditEntry};
use crate::config::ResetMode as ConfigResetMode;
use crate::envelope::{ErrorBody, Warning};
use crate::error::{ErrorCode, PogoError};
use crate::reset::{
    ResetData, enter_bootloader, hard_reset, is_usb_serial_jtag, wait_for_reenumeration,
};
use crate::transport::SerialTransport;

/// How long a native USB board may take to drop off the bus, then to come back.
const VANISH: Duration = Duration::from_secs(1);
const RETURN: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    Run,
    Bootloader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetVia {
    Auto,
    DtrRts,
    Probe,
}

pub fn reset(
    sel: &Selection,
    mode: ResetMode,
    via: ResetVia,
    dry_run: bool,
) -> OpResult<ResetData> {
    let target = match acquire(sel, "reset") {
        Ok(t) => t,
        Err(e) => return OpResult::fail(e),
    };
    let configured_probe =
        target.device_config().and_then(|d| d.reset) == Some(ConfigResetMode::Probe);
    let probe = match via {
        ResetVia::DtrRts => None,
        ResetVia::Probe => Some(true),
        // `auto` follows the device's configured method, else uses the probe when the
        // reset pins are declared and a probe is connected.
        ResetVia::Auto if configured_probe => Some(true),
        ResetVia::Auto if probe_reset::configured(&target.config, mode) => Some(false),
        ResetVia::Auto => None,
    };
    match probe {
        Some(required) => match via_probe(sel, &target, mode, dry_run, required) {
            Some(result) => result,
            None => via_dtr_rts(&target, mode, dry_run),
        },
        None => via_dtr_rts(&target, mode, dry_run),
    }
}

fn mode_name(mode: ResetMode) -> &'static str {
    match mode {
        ResetMode::Run => "run",
        ResetMode::Bootloader => "bootloader",
    }
}

fn audited(
    target: &Target,
    mode: ResetMode,
    via: &str,
    result: &Result<bool, ErrorBody>,
) -> Vec<Warning> {
    if target.replay.is_some() {
        return Vec::new();
    }
    let outcome = match result {
        Ok(_) => "ok",
        Err(e) => e.code.as_str(),
    };
    let d = &target.device;
    let args = serde_json::json!({ "mode": mode_name(mode), "via": via });
    let entry = AuditEntry::new("reset", &d.port, d.usb_serial.as_deref(), args, outcome);
    audit::record(&entry)
        .err()
        .map(|e| Warning::new("AUDIT_FAILED", e))
        .into_iter()
        .collect()
}

fn finish(
    mut data: ResetData,
    result: Result<bool, ErrorBody>,
    warnings: Vec<Warning>,
) -> OpResult<ResetData> {
    match result {
        Ok(reconnected) => {
            data.reconnected = reconnected;
            OpResult::ok(data).with_warnings(warnings)
        }
        Err(e) => OpResult::fail(e).with_warnings(warnings),
    }
}

/// Native USB boards drop off the bus on reset; wait for them to come back.
fn reenumerate(target: &Target) -> Result<bool, PogoError> {
    let d = &target.device;
    match (&d.usb_serial, is_usb_serial_jtag(d.usb_ids)) {
        (Some(serial), true) if target.replay.is_none() => {
            wait_for_reenumeration(serial, VANISH, RETURN)
        }
        _ => Ok(false),
    }
}

/// `None` when the probe is optional (`auto`) and none is connected, so DTR/RTS is used.
fn via_probe(
    sel: &Selection,
    target: &Target,
    mode: ResetMode,
    dry_run: bool,
    required: bool,
) -> Option<OpResult<ResetData>> {
    let pins = match probe_reset::pins(&target.config, mode) {
        Ok(p) => p,
        Err(e) => return Some(OpResult::fail(e)),
    };
    let probe_target = match acquire_probe(sel, target.config.clone(), "reset") {
        Ok(t) => t,
        Err(e) if !required && e.code == ErrorCode::DeviceNotFound => return None,
        Err(e) => return Some(OpResult::fail(e)),
    };
    let data = ResetData {
        device: target.device.clone(),
        mode: mode_name(mode).into(),
        via: "probe".into(),
        reconnected: false,
        dry_run,
    };
    if dry_run {
        return Some(OpResult::ok(data));
    }
    let result = Probe::connect(probe_target)
        .and_then(|mut probe| probe_reset::reset(&mut probe, pins.0, pins.1, mode))
        .and_then(|()| Ok(reenumerate(target)?));
    let warnings = audited(target, mode, "probe", &result);
    Some(finish(data, result, warnings))
}

fn via_dtr_rts(target: &Target, mode: ResetMode, dry_run: bool) -> OpResult<ResetData> {
    let d = &target.device;
    let data = ResetData {
        device: d.clone(),
        mode: mode_name(mode).into(),
        via: "dtr-rts".into(),
        reconnected: false,
        dry_run,
    };
    if dry_run {
        return OpResult::ok(data);
    }
    if let Err(e) = target.require_hardware() {
        return OpResult::fail(e);
    }
    let jtag = is_usb_serial_jtag(d.usb_ids);
    let result = SerialTransport::open(&d.port, target.baud(None))
        .and_then(|mut t| match mode {
            ResetMode::Run => hard_reset(&mut t, jtag),
            ResetMode::Bootloader => enter_bootloader(&mut t, jtag),
        })
        .and_then(|()| reenumerate(target))
        .map_err(ErrorBody::from);
    let warnings = audited(target, mode, "dtr-rts", &result);
    finish(data, result, warnings)
}
