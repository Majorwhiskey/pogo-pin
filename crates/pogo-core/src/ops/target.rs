//! Choosing and locking the target device for a command.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::usage;
use crate::config::{self, Config, ConfigSearch, DeviceConfig};
use crate::discovery::{self, DeviceKind, ResolvedDevice};
use crate::envelope::ErrorBody;
use crate::error::PogoError;
use crate::lock::{self, DeviceLock};
use crate::serial::detect::DEFAULT_FAIL_ON;

pub const DEFAULT_BAUD: u32 = 115_200;

/// How a command picks its device: the adapter fills this from flags or tool arguments.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// `--config` / `POGO_CONFIG`.
    pub config: Option<PathBuf>,
    /// `--device`: alias, USB serial, port, or `auto`.
    pub device: Option<String>,
    /// `--wait-lock`.
    pub wait_lock: Option<Duration>,
    /// `POGO_REPLAY`: a recorded session instead of a device (docs/testing.md).
    pub replay: Option<PathBuf>,
}

/// A locked target. The lock is released when this is dropped.
pub struct Target {
    pub config: Config,
    pub device: ResolvedDevice,
    pub replay: Option<PathBuf>,
    _lock: DeviceLock,
}

impl Target {
    /// The config entry for the device's alias, if it has one.
    pub fn device_config(&self) -> Option<&DeviceConfig> {
        self.config.devices.get(self.device.alias.as_ref()?)
    }

    /// The explicit timeout, else the config's `defaults.timeout`, else `default`.
    pub fn timeout(&self, explicit: Option<Duration>, default: Duration) -> Duration {
        explicit
            .or(self.config.defaults.timeout.map(|t| t.0))
            .unwrap_or(default)
    }

    /// Replays stand in for serial I/O only; flashing needs a real device.
    pub fn require_hardware(&self) -> Result<(), ErrorBody> {
        match &self.replay {
            None => Ok(()),
            Some(_) => Err(usage(
                "POGO_REPLAY only supports serial capture and serial send",
            )),
        }
    }

    pub fn baud(&self, flag: Option<u32>) -> u32 {
        flag.or(self.config.defaults.baud).unwrap_or(DEFAULT_BAUD)
    }

    /// The explicit ELF, else the config's `elf` for the device's alias.
    pub fn elf(&self, explicit: Option<&Path>) -> Option<PathBuf> {
        explicit.map(PathBuf::from).or_else(|| {
            let alias = self.device.alias.as_ref()?;
            self.config.devices.get(alias)?.elf.clone()
        })
    }

    /// Built-in fail-on patterns (unless disabled), then the config's, then the caller's.
    pub fn fail_on(&self, defaults: bool, extra: &[String]) -> Vec<String> {
        let mut all: Vec<String> = Vec::new();
        if defaults {
            all.extend(DEFAULT_FAIL_ON.iter().map(|s| s.to_string()));
        }
        all.extend(self.config.serial.extra_fail_on.iter().cloned());
        all.extend(extra.iter().cloned());
        all
    }
}

/// Config search rooted at the current directory.
pub fn config_search(explicit: Option<PathBuf>) -> Result<ConfigSearch, PogoError> {
    let cwd = std::env::current_dir()
        .map_err(|e| PogoError::Internal(format!("cannot read current directory: {e}")))?;
    Ok(ConfigSearch {
        user: config::user_config_path(),
        cwd,
        explicit,
    })
}

/// Load config, resolve the device, and lock it for `command`.
pub fn acquire(sel: &Selection, command: &str) -> Result<Target, ErrorBody> {
    let loaded = config_search(sel.config.clone()).and_then(|s| config::load(&s))?;
    let device = match &sel.replay {
        // A replay is any device; naming a config alias still applies that alias's settings.
        Some(path) => ResolvedDevice {
            alias: sel
                .device
                .clone()
                .filter(|d| loaded.config.devices.contains_key(d)),
            port: format!("replay:{}", path.display()),
            usb_serial: None,
            kind: DeviceKind::Esp32Target,
            usb_ids: None,
        },
        None => discovery::resolve(
            sel.device.as_deref(),
            "target",
            DeviceKind::Esp32Target,
            discovery::enumerate()?,
            &loaded.config,
        )?,
    };
    let key = device.usb_serial.as_deref().unwrap_or(&device.port);
    let lock = lock::lock_device(key, command, sel.wait_lock)?;
    Ok(Target {
        config: loaded.config,
        device,
        replay: sel.replay.clone(),
        _lock: lock,
    })
}
