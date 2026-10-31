//! Choosing and locking the target device for a command.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::usage;
use crate::config::{self, Config, ConfigSearch, DeviceConfig};
use crate::discovery::{self, DeviceKind, ResolvedDevice};
use crate::envelope::ErrorBody;
use crate::error::{ErrorCode, PogoError};
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
    /// `--probe`: the same forms, for commands that talk to the pogo probe.
    pub probe: Option<String>,
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

/// Load config, resolve the target device, and lock it for `command`.
pub fn acquire(sel: &Selection, command: &str) -> Result<Target, ErrorBody> {
    let config = load_config(sel)?;
    acquire_kind(
        sel,
        config,
        sel.device.as_deref(),
        DeviceKind::Esp32Target,
        command,
    )
}

/// Load config, resolve the pogo probe (`--probe`, default alias `probe`), and lock it.
pub fn acquire_probe(sel: &Selection, config: Config, command: &str) -> Result<Target, ErrorBody> {
    acquire_kind(
        sel,
        config,
        sel.probe.as_deref(),
        DeviceKind::PogoProbe,
        command,
    )
}

pub fn load_config(sel: &Selection) -> Result<Config, ErrorBody> {
    Ok(config_search(sel.config.clone())
        .and_then(|s| config::load(&s))?
        .config)
}

fn acquire_kind(
    sel: &Selection,
    config: Config,
    selector: Option<&str>,
    kind: DeviceKind,
    command: &str,
) -> Result<Target, ErrorBody> {
    let default_alias = match kind {
        DeviceKind::PogoProbe => "probe",
        _ => "target",
    };
    let device = match &sel.replay {
        // A replay is any device; naming a config alias still applies that alias's settings.
        Some(path) => ResolvedDevice {
            alias: selector
                .map(str::to_string)
                .filter(|d| config.devices.contains_key(d)),
            // Distinct from the target's, so a replayed target and probe lock separately.
            port: match kind {
                DeviceKind::PogoProbe => format!("replay-probe:{}", path.display()),
                _ => format!("replay:{}", path.display()),
            },
            usb_serial: None,
            kind,
            usb_ids: None,
        },
        None => discovery::resolve(
            selector,
            default_alias,
            kind,
            discovery::enumerate()?,
            &config,
        )
        .map_err(|e| probe_hint(ErrorBody::from(e), kind))?,
    };
    let key = device.usb_serial.as_deref().unwrap_or(&device.port);
    let lock = lock::lock_device(key, command, sel.wait_lock)?;
    Ok(Target {
        config,
        device,
        replay: sel.replay.clone(),
        _lock: lock,
    })
}

/// The generic selection hints name `--device`; a probe is chosen with `--probe`.
fn probe_hint(mut e: ErrorBody, kind: DeviceKind) -> ErrorBody {
    if kind != DeviceKind::PogoProbe {
        return e;
    }
    match e.code {
        ErrorCode::DeviceNotFound => {
            e.message = e.message.replace("no device", "no pogo probe");
            e.hint = Some(
                "Connect the pogo probe (docs/hardware.md) and run `pogo devices`, then pass \
                 --probe <alias|usb_serial|port>."
                    .into(),
            );
        }
        ErrorCode::AmbiguousDevice => {
            e.hint = Some("Run `pogo devices` and pass --probe <alias|usb_serial|port>.".into());
        }
        _ => {}
    }
    e
}
