//! `pogo reset`.

use std::time::Duration;

use pogo_core::PogoError;
use pogo_core::audit::{self, AuditEntry};
use pogo_core::reset::{
    ResetData, enter_bootloader, hard_reset, is_usb_serial_jtag, wait_for_reenumeration,
};
use pogo_core::transport::SerialTransport;

use crate::cli::{GlobalArgs, ResetMode, ResetVia};
use crate::commands::target;
use crate::output::{Render, Reply, Style};

/// How long a native USB board may take to drop off the bus, then to come back.
const VANISH: Duration = Duration::from_secs(1);
const RETURN: Duration = Duration::from_secs(5);

pub fn reset(global: &GlobalArgs, mode: ResetMode, via: ResetVia, style: &Style) -> Reply {
    if via == ResetVia::Probe {
        return Reply::error(&PogoError::NotImplemented("reset --via probe"));
    }
    let target = match target::acquire(global, "reset") {
        Ok(t) => t,
        Err(reply) => return *reply,
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
        dry_run: global.dry_run,
    };
    if global.dry_run {
        return Reply::ok(&data, style);
    }
    if let Err(reply) = target.require_hardware() {
        return *reply;
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
        Ok(_) => "ok".to_string(),
        Err(e) => e.code().as_str().to_string(),
    };
    let args = serde_json::json!({ "mode": mode_name, "via": "dtr-rts" });
    let entry = AuditEntry::new("reset", &d.port, d.usb_serial.as_deref(), args, &outcome);
    let audit_warning = audit::record(&entry).err();
    match result {
        Ok(reconnected) => {
            data.reconnected = reconnected;
            let mut reply = Reply::ok(&data, style);
            if let Some(e) = audit_warning {
                reply
                    .warnings
                    .push(pogo_core::Warning::new("AUDIT_FAILED", e));
            }
            reply
        }
        Err(e) => Reply::error(&e),
    }
}

impl Render for ResetData {
    fn render_human(&self, out: &mut String, _style: &Style) {
        let verb = if self.dry_run { "would reset" } else { "reset" };
        out.push_str(&format!(
            "{verb} {} into {} mode via {}{}\n",
            self.device.port,
            self.mode,
            self.via,
            if self.reconnected {
                " (re-enumerated)"
            } else {
                ""
            }
        ));
    }
}
