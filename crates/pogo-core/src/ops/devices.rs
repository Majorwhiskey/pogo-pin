//! `devices` as an op. Never opens ports.

use std::path::PathBuf;

use super::OpResult;
use super::target::config_search;
use crate::config;
use crate::discovery::{self, DevicesResult};

/// `config` is the explicit config file, if any (`--config` / `POGO_CONFIG`).
pub fn devices(config: Option<PathBuf>, all: bool) -> OpResult<DevicesResult> {
    let result = config_search(config)
        .and_then(|s| config::load(&s))
        .and_then(|loaded| {
            let ports = discovery::enumerate()?;
            Ok(discovery::list_devices(ports, &loaded.config, all))
        });
    match result {
        Ok(r) => OpResult::ok(r),
        Err(e) => e.into(),
    }
}
