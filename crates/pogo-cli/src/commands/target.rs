//! Shared setup for commands that talk to the target: config, device, lock.

use std::path::PathBuf;

use pogo_core::config::{self, Config};
use pogo_core::discovery::{self, DeviceKind, ResolvedDevice};
use pogo_core::lock::{self, DeviceLock};
use pogo_core::serial::detect::DEFAULT_FAIL_ON;

use pogo_core::{ErrorBody, ErrorCode};

use crate::cli::GlobalArgs;
use crate::commands::config::search;
use crate::output::Reply;

pub const DEFAULT_BAUD: u32 = 115_200;

/// A locked target. The lock is released when this is dropped.
pub struct Target {
    pub config: Config,
    pub device: ResolvedDevice,
    /// Set when `POGO_REPLAY` replaces the device with a recording (docs/testing.md).
    pub replay: Option<PathBuf>,
    _lock: DeviceLock,
}

impl Target {
    /// POGO_REPLAY stands in for serial I/O only; flashing needs a real device.
    pub fn require_hardware(&self) -> Result<(), Box<Reply>> {
        match &self.replay {
            None => Ok(()),
            Some(_) => Err(Box::new(Reply::error(ErrorBody::new(
                ErrorCode::Usage,
                "POGO_REPLAY only supports serial capture and serial send",
            )))),
        }
    }

    pub fn baud(&self, flag: Option<u32>) -> u32 {
        flag.or(self.config.defaults.baud).unwrap_or(DEFAULT_BAUD)
    }

    /// `--elf`, else the config's `elf` for the device's alias.
    pub fn elf(&self, flag: Option<&std::path::Path>) -> Option<PathBuf> {
        flag.map(PathBuf::from).or_else(|| {
            let alias = self.device.alias.as_ref()?;
            self.config.devices.get(alias)?.elf.clone()
        })
    }

    /// Built-in fail-on patterns (unless disabled), then the config's, then the flags'.
    pub fn fail_on(&self, defaults: bool, flags: &[String]) -> Vec<String> {
        let mut all: Vec<String> = Vec::new();
        if defaults {
            all.extend(DEFAULT_FAIL_ON.iter().map(|s| s.to_string()));
        }
        all.extend(self.config.serial.extra_fail_on.iter().cloned());
        all.extend(flags.iter().cloned());
        all
    }
}

/// Load config, resolve `--device`, and lock it for `command`.
pub fn acquire(global: &GlobalArgs, command: &str) -> Result<Target, Box<Reply>> {
    let loaded = search(global.config.clone())
        .and_then(|s| config::load(&s))
        .map_err(|e| Box::new(Reply::error(&e)))?;
    let replay = std::env::var_os("POGO_REPLAY").map(PathBuf::from);
    let device = match &replay {
        Some(path) => ResolvedDevice {
            alias: None,
            port: format!("replay:{}", path.display()),
            usb_serial: None,
            kind: DeviceKind::Esp32Target,
            usb_ids: None,
        },
        None => discovery::enumerate()
            .and_then(|ports| {
                discovery::resolve(
                    global.device.as_deref(),
                    "target",
                    DeviceKind::Esp32Target,
                    ports,
                    &loaded.config,
                )
            })
            .map_err(|e| Box::new(Reply::error(&e)))?,
    };
    let key = device.usb_serial.as_deref().unwrap_or(&device.port);
    let lock = lock::lock_device(key, command, global.wait_lock)
        .map_err(|e| Box::new(Reply::error(&e)))?;
    Ok(Target {
        config: loaded.config,
        device,
        replay,
        _lock: lock,
    })
}
