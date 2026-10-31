//! `pogo doctor` checks (docs/commands.md §7.2). Each check is a pure function of facts
//! gathered elsewhere, so it can be tested without hardware or a particular OS.

use schemars::JsonSchema;
use serde::Serialize;

use crate::config::LoadedConfig;
use crate::discovery::{DeviceKind, DevicesResult};
use crate::error::PogoError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Check {
    /// Stable id: `config`, `devices`, `permissions`, `modemmanager`, `probe`, `open`.
    pub id: String,
    pub status: CheckStatus,
    pub message: String,
    pub hint: Option<String>,
}

impl Check {
    fn new(id: &str, status: CheckStatus, message: impl Into<String>) -> Self {
        Check {
            id: id.into(),
            status,
            message: message.into(),
            hint: None,
        }
    }

    fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// `data` for `pogo doctor`. Checks that do not apply on this OS are left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DoctorData {
    pub checks: Vec<Check>,
}

pub fn config_check(loaded: &Result<LoadedConfig, PogoError>) -> Check {
    match loaded {
        Ok(l) => {
            let files: Vec<String> = l.loaded_paths().map(|p| p.display().to_string()).collect();
            let msg = if files.is_empty() {
                "No config files found; built-in defaults apply".to_string()
            } else {
                format!("Loaded {}", files.join(", "))
            };
            Check::new("config", CheckStatus::Pass, msg)
        }
        Err(e) => Check::new("config", CheckStatus::Fail, e.to_string())
            .hint("Run `pogo config validate` to locate the problem."),
    }
}

pub fn devices_check(devices: &Result<DevicesResult, PogoError>) -> Check {
    let devices = match devices {
        Ok(d) => d,
        Err(e) => return Check::new("devices", CheckStatus::Fail, e.to_string()),
    };
    let boards: Vec<&str> = devices
        .devices
        .iter()
        .filter(|d| d.kind == DeviceKind::Esp32Target)
        .map(|d| d.port.as_str())
        .collect();
    match boards.as_slice() {
        [] => Check::new("devices", CheckStatus::Warn, "No ESP32 boards visible").hint(
            "Plug in a board. If it still does not appear, try another cable (many are \
             charge-only) and check `pogo devices --all`.",
        ),
        [one] => Check::new(
            "devices",
            CheckStatus::Pass,
            format!("1 ESP32 board: {one}"),
        ),
        many => Check::new(
            "devices",
            CheckStatus::Pass,
            format!("{} ESP32 boards: {}", many.len(), many.join(", ")),
        )
        .hint("Several boards are connected; pass --device or set a config alias."),
    }
}

pub fn probe_check(devices: &Result<DevicesResult, PogoError>) -> Check {
    let probes: Vec<&str> = devices
        .as_ref()
        .map(|d| {
            d.devices
                .iter()
                .filter(|d| d.kind == DeviceKind::PogoProbe)
                .map(|d| d.port.as_str())
                .collect()
        })
        .unwrap_or_default();
    match probes.first() {
        Some(port) => Check::new("probe", CheckStatus::Pass, format!("pogo probe on {port}")),
        None => Check::new("probe", CheckStatus::Warn, "No pogo probe connected")
            .hint("Only gpio, i2c, power, and probe commands need one (docs/hardware.md)."),
    }
}

/// A serial port's mode and group, against the groups this user belongs to (Linux).
pub fn permission_check(port: &str, mode: u32, gid: u32, group: &str, user_gids: &[u32]) -> Check {
    let world_rw = mode & 0o006 == 0o006;
    let group_rw = mode & 0o060 == 0o060 && user_gids.contains(&gid);
    if world_rw || group_rw {
        Check::new(
            "permissions",
            CheckStatus::Pass,
            format!("{port} is accessible"),
        )
    } else {
        Check::new(
            "permissions",
            CheckStatus::Fail,
            format!("{port} belongs to group '{group}', which this user is not in"),
        )
        .hint(format!(
            "sudo usermod -aG {group} $USER, then log out and back in"
        ))
    }
}

pub fn modemmanager_check(running: bool) -> Check {
    if running {
        Check::new("modemmanager", CheckStatus::Warn, "ModemManager is running").hint(
            "It can grab /dev/ttyACM* ports as they appear; add the udev rule in \
             docs/hardware.md or stop ModemManager.",
        )
    } else {
        Check::new(
            "modemmanager",
            CheckStatus::Pass,
            "ModemManager is not running",
        )
    }
}

/// `--deep`: the result of opening a board's port.
pub fn open_check(port: &str, result: Result<(), PogoError>) -> Check {
    match result {
        Ok(()) => Check::new("open", CheckStatus::Pass, format!("Opened {port}")),
        Err(e) => {
            let hint = e.hint();
            let check = Check::new("open", CheckStatus::Fail, e.to_string());
            match hint {
                Some(h) => check.hint(h),
                None => check,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::DeviceInfo;

    fn listed(kinds: &[(DeviceKind, &str)]) -> Result<DevicesResult, PogoError> {
        Ok(DevicesResult {
            devices: kinds
                .iter()
                .map(|(kind, port)| DeviceInfo {
                    alias: None,
                    kind: *kind,
                    port: port.to_string(),
                    usb: None,
                    chip_hint: None,
                    locked_by: None,
                })
                .collect(),
        })
    }

    #[test]
    fn devices_and_probe() {
        let none = listed(&[]);
        assert_eq!(devices_check(&none).status, CheckStatus::Warn);
        assert_eq!(probe_check(&none).status, CheckStatus::Warn);
        let two = listed(&[
            (DeviceKind::Esp32Target, "/dev/a"),
            (DeviceKind::Esp32Target, "/dev/b"),
            (DeviceKind::PogoProbe, "/dev/p"),
        ]);
        let d = devices_check(&two);
        assert_eq!(
            (d.status, d.message.as_str()),
            (CheckStatus::Pass, "2 ESP32 boards: /dev/a, /dev/b")
        );
        assert!(d.hint.is_some());
        assert_eq!(probe_check(&two).message, "pogo probe on /dev/p");
        let err = Err(PogoError::DeviceIo("no ports".into()));
        assert_eq!(devices_check(&err).status, CheckStatus::Fail);
    }

    #[test]
    fn permissions_by_mode_and_group() {
        let dialout = 20;
        assert_eq!(
            permission_check("/dev/ttyACM0", 0o660, dialout, "dialout", &[1000, 20]).status,
            CheckStatus::Pass
        );
        assert_eq!(
            permission_check("/dev/ttyACM0", 0o666, dialout, "dialout", &[1000]).status,
            CheckStatus::Pass
        );
        let fail = permission_check("/dev/ttyACM0", 0o660, dialout, "dialout", &[1000]);
        assert_eq!(fail.status, CheckStatus::Fail);
        assert!(fail.hint.unwrap().contains("usermod -aG dialout"));
        assert_eq!(
            permission_check("/dev/x", 0o600, 20, "dialout", &[20]).status,
            CheckStatus::Fail
        );
    }

    #[test]
    fn modemmanager_and_open() {
        assert_eq!(modemmanager_check(true).status, CheckStatus::Warn);
        assert_eq!(modemmanager_check(false).status, CheckStatus::Pass);
        let denied = open_check(
            "/dev/x",
            Err(PogoError::PermissionDenied {
                port: "/dev/x".into(),
            }),
        );
        assert_eq!(denied.status, CheckStatus::Fail);
        assert!(denied.hint.is_some());
        assert_eq!(open_check("/dev/x", Ok(())).status, CheckStatus::Pass);
    }

    #[test]
    fn config_failure_fails() {
        let c = config_check(&Err(PogoError::Config("bad".into())));
        assert_eq!(c.status, CheckStatus::Fail);
        let json = serde_json::to_value(&c).unwrap();
        assert_eq!(json["status"], "fail");
        assert_eq!(json["id"], "config");
    }
}
