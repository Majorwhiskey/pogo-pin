//! Config discovery, layered merge with per-key sources, and validation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::Serialize;
use toml::{Table, Value};

use super::{CONFIG_SCHEMA, Config, parse_addr_range};
use crate::error::PogoError;

/// Where to look for config files. Built by the caller so the core stays free of
/// process-global state.
#[derive(Debug, Clone)]
pub struct ConfigSearch {
    /// User-level config file, usually from [`user_config_path`].
    pub user: Option<PathBuf>,
    /// Directory to start the upward `pogo.toml` search from.
    pub cwd: PathBuf,
    /// File from `--config` or `POGO_CONFIG`; must exist when given.
    pub explicit: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFileKind {
    User,
    Project,
    Explicit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ConfigFile {
    pub path: PathBuf,
    pub kind: ConfigFileKind,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct KeySource {
    /// Dotted key, e.g. `devices.target.chip`.
    pub key: String,
    /// File that supplied the winning value.
    pub source: PathBuf,
}

/// `data` for `pogo config path`: candidate files in merge order.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConfigPathResult {
    pub files: Vec<ConfigFile>,
}

/// `data` for `pogo config show`: values that are set, and where each came from.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConfigShowResult {
    pub config: serde_json::Value,
    pub sources: Vec<KeySource>,
}

/// `data` for `pogo config validate`.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConfigValidateResult {
    /// Files that were loaded, in merge order.
    pub files: Vec<PathBuf>,
    pub valid: bool,
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub files: Vec<ConfigFile>,
    pub merged: Table,
    pub sources: BTreeMap<String, PathBuf>,
    pub config: Config,
}

impl LoadedConfig {
    pub fn show(&self) -> ConfigShowResult {
        ConfigShowResult {
            config: serde_json::to_value(&self.merged).unwrap_or_default(),
            sources: self
                .sources
                .iter()
                .map(|(key, source)| KeySource {
                    key: key.clone(),
                    source: source.clone(),
                })
                .collect(),
        }
    }

    pub fn validated(&self) -> ConfigValidateResult {
        ConfigValidateResult {
            files: self.loaded_paths().cloned().collect(),
            valid: true,
        }
    }

    pub fn loaded_paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.files.iter().filter(|f| f.exists).map(|f| &f.path)
    }
}

/// The platform user config file, e.g. `~/.config/pogo/config.toml` on Linux.
pub fn user_config_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| d.config_dir().join("pogo").join("config.toml"))
}

/// Candidate config files in merge order (later wins). Never fails.
pub fn discover(search: &ConfigSearch) -> Vec<ConfigFile> {
    let mut files = Vec::new();
    if let Some(user) = &search.user {
        files.push(ConfigFile {
            path: user.clone(),
            kind: ConfigFileKind::User,
            exists: user.is_file(),
        });
    }
    let project = search
        .cwd
        .ancestors()
        .map(|dir| dir.join("pogo.toml"))
        .find(|p| p.is_file());
    files.push(ConfigFile {
        exists: project.is_some(),
        path: project.unwrap_or_else(|| search.cwd.join("pogo.toml")),
        kind: ConfigFileKind::Project,
    });
    if let Some(explicit) = &search.explicit {
        let path = search.cwd.join(explicit);
        files.push(ConfigFile {
            exists: path.is_file(),
            path,
            kind: ConfigFileKind::Explicit,
        });
    }
    files
}

/// Discover, parse, merge, and validate config. Missing user/project files are fine;
/// a missing explicit file is an error.
pub fn load(search: &ConfigSearch) -> Result<LoadedConfig, PogoError> {
    let files = discover(search);
    let mut merged = Table::new();
    let mut sources = BTreeMap::new();
    for file in &files {
        if !file.exists {
            if file.kind == ConfigFileKind::Explicit {
                return Err(PogoError::Config(format!(
                    "config file not found: {}",
                    file.path.display()
                )));
            }
            continue;
        }
        let table = parse_file(&file.path)?;
        merge(&mut merged, table, &file.path, "", &mut sources);
    }
    let config: Config = Value::Table(merged.clone())
        .try_into()
        .map_err(|e: toml::de::Error| PogoError::Config(format!("merged config: {e}")))?;
    validate(&config, &sources)?;
    Ok(LoadedConfig {
        files,
        merged,
        sources,
        config,
    })
}

/// Parse one file, checking syntax, known keys, and types so errors carry a line number.
fn parse_file(path: &Path) -> Result<Table, PogoError> {
    let in_file = |msg: String| PogoError::Config(format!("{}: {}", path.display(), msg));
    let text = std::fs::read_to_string(path).map_err(|e| in_file(e.to_string()))?;
    let typed: Config =
        toml::from_str(&text).map_err(|e| in_file(e.to_string().trim_end().to_string()))?;
    if let Some(v) = typed.schema.filter(|v| *v != CONFIG_SCHEMA) {
        return Err(in_file(format!(
            "unsupported config schema {v}; this pogo reads schema {CONFIG_SCHEMA}"
        )));
    }
    toml::from_str(&text).map_err(|e| in_file(e.to_string().trim_end().to_string()))
}

/// Deep-merge `src` into `dst`. Tables merge key by key; any other value replaces the old
/// one wholesale. `match` tables are replaced whole, since mixing selectors from two
/// files would describe a device neither file meant.
fn merge(
    dst: &mut Table,
    src: Table,
    source: &Path,
    prefix: &str,
    sources: &mut BTreeMap<String, PathBuf>,
) {
    for (k, v) in src {
        let key = join_key(prefix, &k);
        match (dst.get_mut(&k), v) {
            (Some(Value::Table(d)), Value::Table(s)) if k != "match" => {
                merge(d, s, source, &key, sources);
            }
            (_, v) => {
                let nested = format!("{key}.");
                sources.retain(|s, _| s != &key && !s.starts_with(&nested));
                record_leaves(&key, &k, &v, source, sources);
                dst.insert(k, v);
            }
        }
    }
}

fn record_leaves(
    key: &str,
    name: &str,
    v: &Value,
    source: &Path,
    sources: &mut BTreeMap<String, PathBuf>,
) {
    match v {
        Value::Table(t) if name != "match" => {
            for (k, v) in t {
                record_leaves(&join_key(key, k), k, v, source, sources);
            }
        }
        _ => {
            sources.insert(key.to_string(), source.to_path_buf());
        }
    }
}

fn join_key(prefix: &str, k: &str) -> String {
    if prefix.is_empty() {
        k.to_string()
    } else {
        format!("{prefix}.{k}")
    }
}

/// Checks that need the merged view (required fields) or more than serde (regexes).
fn validate(config: &Config, sources: &BTreeMap<String, PathBuf>) -> Result<(), PogoError> {
    let fail = |key: &str, msg: &str| {
        let nested = format!("{key}.");
        let src = sources.get(key).or_else(|| {
            sources
                .iter()
                .find(|(k, _)| k.starts_with(&nested))
                .map(|(_, p)| p)
        });
        PogoError::Config(match src {
            Some(path) => format!("{}: {key}: {msg}", path.display()),
            None => format!("{key}: {msg}"),
        })
    };
    for (name, device) in &config.devices {
        let key = format!("devices.{name}");
        if device.kind.is_none() {
            return Err(fail(&key, "missing `kind` (esp32 | pogo-probe)"));
        }
        match &device.matcher {
            None => return Err(fail(&key, "missing `match`")),
            Some(m) if m.usb_serial.is_none() && m.port.is_none() && m.vid.is_none() => {
                return Err(fail(
                    &format!("{key}.match"),
                    "needs at least one of usb_serial, port, vid",
                ));
            }
            Some(_) => {}
        }
    }
    for (name, pin) in &config.pins {
        if pin.probe_pin.is_none() {
            return Err(fail(&format!("pins.{name}"), "missing `probe_pin`"));
        }
    }
    for pattern in &config.serial.extra_fail_on {
        if let Err(e) = regex::Regex::new(pattern) {
            let msg = format!("'{pattern}' is not a valid regex: {e}");
            return Err(fail("serial.extra_fail_on", &msg));
        }
    }
    for addr in config.guardrails.i2c_protected_addrs.iter().flatten() {
        if parse_addr_range(addr).is_none() {
            let msg = format!("'{addr}' is not an address like 0x50 or a range like 0x50-0x57");
            return Err(fail("guardrails.i2c_protected_addrs", &msg));
        }
    }
    Ok(())
}
