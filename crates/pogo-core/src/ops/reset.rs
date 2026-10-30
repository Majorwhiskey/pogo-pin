//! `reset` as an op.

use std::time::Duration;

use super::OpResult;
use super::target::{Selection, acquire};
use crate::audit::{self, AuditEntry};
use crate::envelope::Warning;
use crate::error::PogoError;
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
    if via == ResetVia::Probe {
        return PogoError::NotImplemented("reset via probe").into();
    }
    let target = match acquire(sel, "reset") {
        Ok(t) => t,
        Err(e) => return OpResult::fail(e),
    };
    let mode_name = match mode {
        ResetMode::Run => "run",
        ResetMode::Bootloader => "bootloader",
    };
    let d = &target.device;
    let mut data = ResetData {
        device: d.clone(),
        mode: mode_name.into(),
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
        .and_then(|()| match (&d.usb_serial, jtag) {
            (Some(serial), true) => wait_for_reenumeration(serial, VANISH, RETURN),
            _ => Ok(false),
        });
    let outcome = match &result {
        Ok(_) => "ok",
        Err(e) => e.code().as_str(),
    };
    let args = serde_json::json!({ "mode": mode_name, "via": "dtr-rts" });
    let entry = AuditEntry::new("reset", &d.port, d.usb_serial.as_deref(), args, outcome);
    let warnings: Vec<Warning> = audit::record(&entry)
        .err()
        .map(|e| Warning::new("AUDIT_FAILED", e))
        .into_iter()
        .collect();
    match result {
        Ok(reconnected) => {
            data.reconnected = reconnected;
            OpResult::ok(data).with_warnings(warnings)
        }
        Err(e) => OpResult::fail(&e).with_warnings(warnings),
    }
}
