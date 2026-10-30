use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;
use crate::error::{ErrorCode, PogoError};

fn scratch(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("pogo-config-{name}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, text: &str) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
    // Native separators, so `display()` matches paths built by discovery on Windows.
    path.components().collect()
}

fn search(dir: &Path) -> ConfigSearch {
    ConfigSearch {
        user: Some(dir.join("user/config.toml")),
        cwd: dir.join("proj"),
        explicit: None,
    }
}

fn config_err(r: Result<LoadedConfig, PogoError>) -> String {
    match r {
        Err(PogoError::Config(msg)) => msg,
        other => panic!("expected config error, got {other:?}"),
    }
}

#[test]
fn no_files_is_an_empty_valid_config() {
    let dir = scratch("empty");
    std::fs::create_dir_all(dir.join("proj")).unwrap();
    let loaded = load(&search(&dir)).unwrap();
    assert_eq!(loaded.config, Config::default());
    assert!(loaded.sources.is_empty());
    assert_eq!(loaded.loaded_paths().count(), 0);
}

#[test]
fn later_files_win_and_sources_track_each_key() {
    let dir = scratch("merge");
    let user = write(
        &dir.join("user/config.toml"),
        r#"
        [defaults]
        format = "json"
        baud = 9600
        [devices.target]
        kind = "esp32"
        chip = "esp32c3"
        match = { usb_serial = "AA:BB" }
        "#,
    );
    let project = write(
        &dir.join("proj/pogo.toml"),
        r#"
        schema = 1
        [defaults]
        baud = 115200
        [devices.target]
        match = { port = "/dev/cu.usbmodem1" }
        "#,
    );
    let explicit = write(
        &dir.join("bench.toml"),
        "[devices.target]\nchip = \"esp32c6\"\n",
    );
    let mut s = search(&dir);
    s.explicit = Some(explicit.clone());
    let loaded = load(&s).unwrap();

    assert_eq!(loaded.config.defaults.baud, Some(115200));
    assert_eq!(loaded.config.defaults.format, Some(OutputFormat::Json));
    let target = &loaded.config.devices["target"];
    assert_eq!(target.chip.as_deref(), Some("esp32c6"));
    // `match` is replaced whole, not merged field by field.
    let m = target.matcher.clone().unwrap();
    assert_eq!(m.port.as_deref(), Some("/dev/cu.usbmodem1"));
    assert_eq!(m.usb_serial, None);

    let src = |k: &str| loaded.sources.get(k).cloned();
    assert_eq!(src("defaults.format"), Some(user.clone()));
    assert_eq!(src("defaults.baud"), Some(project.clone()));
    assert_eq!(src("devices.target.kind"), Some(user));
    assert_eq!(src("devices.target.match"), Some(project));
    assert_eq!(src("devices.target.chip"), Some(explicit));
    assert_eq!(src("devices.target.match.port"), None);
    assert_eq!(loaded.show().sources.len(), 6);
}

#[test]
fn project_file_is_found_from_a_subdirectory() {
    let dir = scratch("walk");
    let project = write(&dir.join("proj/pogo.toml"), "[defaults]\nbaud = 1\n");
    let mut s = search(&dir);
    s.cwd = dir.join("proj/a/b");
    std::fs::create_dir_all(&s.cwd).unwrap();
    let files = discover(&s);
    assert_eq!(files[1].kind, ConfigFileKind::Project);
    assert_eq!(files[1].path, project);
    assert!(files[1].exists);
}

#[test]
fn missing_explicit_file_is_an_error() {
    let dir = scratch("explicit");
    let mut s = search(&dir);
    s.explicit = Some(dir.join("nope.toml"));
    let err = load(&s).unwrap_err();
    assert_eq!(err.code(), ErrorCode::ConfigInvalid);
    assert!(err.to_string().contains("nope.toml"));
}

#[test]
fn unknown_key_reports_file_and_line() {
    let dir = scratch("unknown");
    write(
        &dir.join("proj/pogo.toml"),
        "[defaults]\nbaud = 1\nbuad = 2\n",
    );
    let msg = config_err(load(&search(&dir)));
    assert!(msg.contains("pogo.toml"), "{msg}");
    assert!(msg.contains("line 3"), "{msg}");
    assert!(msg.contains("buad"), "{msg}");
}

#[test]
fn wrong_type_and_bad_enum_are_rejected() {
    for text in [
        "[defaults]\nbaud = \"fast\"\n",
        "[defaults]\nformat = \"yaml\"\n",
        "[defaults]\ntimeout = \"soon\"\n",
        "[devices.t]\nreset = \"sometimes\"\n",
    ] {
        let dir = scratch("types");
        write(&dir.join("proj/pogo.toml"), text);
        config_err(load(&search(&dir)));
    }
}

#[test]
fn unsupported_schema_is_rejected() {
    let dir = scratch("schema");
    write(&dir.join("proj/pogo.toml"), "schema = 2\n");
    let msg = config_err(load(&search(&dir)));
    assert!(msg.contains("unsupported config schema 2"), "{msg}");
}

#[test]
fn device_needs_kind_and_match_after_merge() {
    let dir = scratch("device");
    let project = write(
        &dir.join("proj/pogo.toml"),
        "[devices.target]\nmatch = { vid = 0x303a }\n",
    );
    let msg = config_err(load(&search(&dir)));
    assert!(
        msg.starts_with(&format!(
            "{}: devices.target: missing `kind`",
            project.display()
        )),
        "{msg}"
    );

    write(
        &dir.join("user/config.toml"),
        "[devices.target]\nkind = \"esp32\"\n",
    );
    assert!(load(&search(&dir)).is_ok());

    write(
        &dir.join("proj/pogo.toml"),
        "[devices.target]\nmatch = {}\n",
    );
    let msg = config_err(load(&search(&dir)));
    assert!(
        msg.contains("devices.target.match: needs at least one"),
        "{msg}"
    );
}

#[test]
fn pin_needs_probe_pin() {
    let dir = scratch("pin");
    write(
        &dir.join("proj/pogo.toml"),
        "[pins.led]\nmode = \"push-pull\"\n",
    );
    let msg = config_err(load(&search(&dir)));
    assert!(msg.contains("pins.led: missing `probe_pin`"), "{msg}");
}

#[test]
fn bad_regex_and_bad_address_are_rejected() {
    let dir = scratch("regex");
    write(
        &dir.join("proj/pogo.toml"),
        "[serial]\nextra_fail_on = [\"(\"]\n",
    );
    let msg = config_err(load(&search(&dir)));
    assert!(
        msg.contains("serial.extra_fail_on: '(' is not a valid regex"),
        "{msg}"
    );

    write(
        &dir.join("proj/pogo.toml"),
        "[guardrails]\ni2c_protected_addrs = [\"0x57-0x50\"]\n",
    );
    let msg = config_err(load(&search(&dir)));
    assert!(msg.contains("'0x57-0x50' is not an address"), "{msg}");
}

#[test]
fn address_ranges() {
    assert_eq!(parse_addr_range("0x50-0x57"), Some((0x50, 0x57)));
    assert_eq!(parse_addr_range("0x76"), Some((0x76, 0x76)));
    assert_eq!(parse_addr_range(" 0X10 - 0x1f "), Some((0x10, 0x1f)));
    assert_eq!(parse_addr_range("0x80"), None);
    assert_eq!(parse_addr_range("80"), None);
    assert_eq!(parse_addr_range("0x57-0x50"), None);
}

#[test]
fn example_config_loads() {
    let dir = scratch("example");
    write(
        &dir.join("proj/pogo.toml"),
        include_str!("../../../../pogo.example.toml"),
    );
    let loaded = load(&search(&dir)).unwrap();
    let c = &loaded.config;
    assert_eq!(
        c.defaults.timeout,
        Some(ConfigDuration(Duration::from_secs(10)))
    );
    assert_eq!(c.devices["target"].kind, Some(DeviceType::Esp32));
    assert_eq!(c.devices["probe"].kind, Some(DeviceType::PogoProbe));
    assert_eq!(c.pins["target_en"].probe_pin, Some(2));
    assert_eq!(c.power.as_ref().and_then(|p| p.ina226_addr), Some(0x40));
    assert_eq!(
        c.guardrails.min_power_off_time,
        Some(ConfigDuration(Duration::from_millis(200)))
    );
}

#[test]
fn relative_elf_resolves_against_its_config_file() {
    let dir = scratch("elf");
    write(
        &dir.join("proj/pogo.toml"),
        "[devices.target]\nkind = \"esp32\"\nmatch = { vid = 0x303a }\nelf = \"fw/app.elf\"\n",
    );
    let mut s = search(&dir);
    s.cwd = dir.join("proj/sub");
    std::fs::create_dir_all(&s.cwd).unwrap();
    let loaded = load(&s).unwrap();
    let elf = loaded.config.devices["target"].elf.clone().unwrap();
    assert_eq!(elf, dir.join("proj").join("fw/app.elf"));

    // Absolute on every OS (a drive letter on Windows); forward slashes keep TOML simple.
    let abs = dir
        .join("elsewhere")
        .join("app.elf")
        .display()
        .to_string()
        .replace('\\', "/");
    write(
        &dir.join("proj/pogo.toml"),
        &format!(
            "[devices.target]\nkind = \"esp32\"\nmatch = {{ vid = 0x303a }}\nelf = \"{abs}\"\n"
        ),
    );
    let elf = load(&s).unwrap().config.devices["target"]
        .elf
        .clone()
        .unwrap();
    assert_eq!(elf, std::path::PathBuf::from(&abs));
}
