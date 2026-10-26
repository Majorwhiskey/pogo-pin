//! Configuration files: discovery, merge, and validation (docs/configuration.md §10).

mod load;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Deserializer};

pub use load::{
    ConfigFile, ConfigFileKind, ConfigPathResult, ConfigSearch, ConfigShowResult,
    ConfigValidateResult, KeySource, LoadedConfig, discover, load, user_config_path,
};

/// Config schema version this build reads.
pub const CONFIG_SCHEMA: u32 = 1;

/// Typed view of a config file. Every field is optional so that a single file can hold
/// part of the config; completeness is checked after merging.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: Option<u32>,
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub devices: BTreeMap<String, DeviceConfig>,
    #[serde(default)]
    pub pins: BTreeMap<String, PinConfig>,
    pub power: Option<PowerConfig>,
    #[serde(default)]
    pub serial: SerialConfig,
    #[serde(default)]
    pub guardrails: GuardrailsConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    pub format: Option<OutputFormat>,
    pub baud: Option<u32>,
    pub timeout: Option<ConfigDuration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Human,
    Json,
    Jsonl,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceConfig {
    pub kind: Option<DeviceType>,
    #[serde(rename = "match")]
    pub matcher: Option<DeviceMatch>,
    pub chip: Option<String>,
    pub elf: Option<PathBuf>,
    pub reset: Option<ResetMode>,
    pub parser: Option<ParserName>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceType {
    Esp32,
    PogoProbe,
}

/// How to recognize a device. At least one field must be set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceMatch {
    pub usb_serial: Option<String>,
    pub port: Option<String>,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResetMode {
    Auto,
    DtrRts,
    Probe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ParserName {
    Auto,
    EspIdf,
    RustLog,
    Raw,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinConfig {
    pub probe_pin: Option<u8>,
    pub mode: Option<PinMode>,
    pub active: Option<ActiveLevel>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PinMode {
    OpenDrain,
    PushPull,
    Input,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActiveLevel {
    Low,
    High,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerConfig {
    pub ina226_addr: Option<u8>,
    pub shunt_ohms: Option<f64>,
    pub switch_pin: Option<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerialConfig {
    #[serde(default)]
    pub extra_fail_on: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardrailsConfig {
    pub i2c_protected_addrs: Option<Vec<String>>,
    pub min_power_off_time: Option<ConfigDuration>,
    pub max_power_cycles_per_minute: Option<u32>,
}

/// A duration written as a human string such as `500ms` or `10s`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigDuration(pub Duration);

impl<'de> Deserialize<'de> for ConfigDuration {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        humantime::parse_duration(&s)
            .map(ConfigDuration)
            .map_err(|e| serde::de::Error::custom(format!("invalid duration '{s}': {e}")))
    }
}

/// Parse one `i2c_protected_addrs` entry: `0x50` or an inclusive range `0x50-0x57`.
pub fn parse_addr_range(s: &str) -> Option<(u8, u8)> {
    let hex = |p: &str| {
        let p = p.trim();
        let digits = p.strip_prefix("0x").or_else(|| p.strip_prefix("0X"))?;
        u8::from_str_radix(digits, 16).ok().filter(|a| *a <= 0x7f)
    };
    let (lo, hi) = match s.split_once('-') {
        Some((a, b)) => (hex(a)?, hex(b)?),
        None => {
            let a = hex(s)?;
            (a, a)
        }
    };
    (lo <= hi).then_some((lo, hi))
}
