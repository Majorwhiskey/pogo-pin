//! `doctor` as an op: gather facts, run the checks in `crate::doctor`.

use std::path::PathBuf;

use super::OpResult;
use super::target::config_search;
use crate::config;
use crate::discovery::{self, DeviceKind, DevicesResult};
use crate::doctor::{
    Check, CheckStatus, DoctorData, config_check, devices_check, open_check, probe_check,
};
use crate::envelope::ErrorBody;
use crate::error::{ErrorCode, PogoError};
use crate::lock;
use crate::transport::SerialTransport;

/// `deep` also opens each board's port (without resetting it).
pub fn doctor(config: Option<PathBuf>, deep: bool) -> OpResult<DoctorData> {
    let loaded = config_search(config).and_then(|s| config::load(&s));
    let mut checks = vec![config_check(&loaded)];
    let cfg = loaded
        .as_ref()
        .map(|l| l.config.clone())
        .unwrap_or_default();
    let devices = discovery::enumerate().map(|ports| discovery::list_devices(ports, &cfg, true));
    checks.push(devices_check(&devices));
    #[cfg(target_os = "linux")]
    {
        checks.extend(linux::permission_checks(&devices));
        checks.push(crate::doctor::modemmanager_check(
            linux::modemmanager_running(),
        ));
    }
    checks.push(probe_check(&devices));
    if deep {
        checks.extend(open_checks(&devices));
    }

    let error = if loaded.is_err() {
        loaded.err().map(ErrorBody::from)
    } else {
        let failed: Vec<&str> = checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .map(|c| c.id.as_str())
            .collect();
        (!failed.is_empty()).then(|| {
            ErrorBody::new(
                ErrorCode::CheckFailed,
                format!("{} check(s) failed: {}", failed.len(), failed.join(", ")),
            )
            .with_hint("See each failing check's hint in data.checks.")
        })
    };
    OpResult::ok(DoctorData { checks }).with_error(error)
}

/// Lock and open each board's port, the way a real command would.
fn open_checks(devices: &Result<DevicesResult, PogoError>) -> Vec<Check> {
    let Ok(devices) = devices else {
        return Vec::new();
    };
    devices
        .devices
        .iter()
        .filter(|d| d.kind == DeviceKind::Esp32Target)
        .map(|d| {
            let key = d
                .usb
                .as_ref()
                .and_then(|u| u.serial_number.clone())
                .unwrap_or_else(|| d.port.clone());
            let result = lock::lock_device(&key, "doctor", None)
                .and_then(|_lock| SerialTransport::open(&d.port, 115_200).map(drop));
            open_check(&d.port, result)
        })
        .collect()
}

#[cfg(target_os = "linux")]
mod linux {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    use crate::discovery::{DeviceKind, DevicesResult};
    use crate::doctor::{Check, permission_check};
    use crate::error::PogoError;

    pub fn permission_checks(devices: &Result<DevicesResult, PogoError>) -> Vec<Check> {
        let Ok(devices) = devices else {
            return Vec::new();
        };
        let user_gids = user_gids();
        devices
            .devices
            .iter()
            .filter(|d| d.kind == DeviceKind::Esp32Target)
            .filter_map(|d| {
                let meta = std::fs::metadata(&d.port).ok()?;
                let gid = meta.gid();
                let group = group_name(gid).unwrap_or_else(|| gid.to_string());
                let mode = meta.permissions().mode();
                Some(permission_check(&d.port, mode, gid, &group, &user_gids))
            })
            .collect()
    }

    /// Supplementary groups of this process, from `id -G`.
    fn user_gids() -> Vec<u32> {
        std::process::Command::new("id")
            .arg("-G")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| {
                s.split_whitespace()
                    .filter_map(|g| g.parse().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn group_name(gid: u32) -> Option<String> {
        let groups = std::fs::read_to_string("/etc/group").ok()?;
        groups.lines().find_map(|line| {
            let mut fields = line.split(':');
            let name = fields.next()?;
            let id: u32 = fields.nth(1)?.parse().ok()?;
            (id == gid).then(|| name.to_string())
        })
    }

    pub fn modemmanager_running() -> bool {
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return false;
        };
        entries.flatten().any(|e| {
            e.file_name()
                .to_string_lossy()
                .bytes()
                .all(|b| b.is_ascii_digit())
                && std::fs::read_to_string(e.path().join("comm"))
                    .is_ok_and(|c| c.trim() == "ModemManager")
        })
    }
}
