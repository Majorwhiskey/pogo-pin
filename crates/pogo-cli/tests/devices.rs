//! `pogo devices`: envelope and exit codes. Runs against whatever ports the host has.

#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
use serde_json::Value;

fn pogo() -> Command {
    let mut cmd = Command::cargo_bin("pogo").unwrap();
    cmd.env_remove("POGO_FORMAT")
        .env_remove("POGO_CONFIG")
        .env("NO_COLOR", "1");
    cmd
}

#[test]
fn json_envelope_lists_devices() {
    let out = pogo()
        .args(["devices", "--all", "-f", "json"])
        .assert()
        .code(0)
        .get_output()
        .clone();
    assert!(out.stderr.is_empty());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["command"], "devices");
    assert_eq!(v["ok"], true);
    for d in v["data"]["devices"].as_array().unwrap() {
        assert!(d["port"].is_string());
        let kind = d["kind"].as_str().unwrap();
        assert!(["esp32_target", "pogo_probe", "unknown_serial"].contains(&kind));
        assert!(d.get("locked_by").is_some());
    }
}

#[test]
fn invalid_config_exits_10() {
    let dir = std::env::temp_dir().join(format!("pogo-devices-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.toml");
    std::fs::write(&bad, "[devices.x]\nkind = \"esp8266\"\n").unwrap();
    let out = pogo()
        .args(["devices", "-f", "json", "--config"])
        .arg(&bad)
        .assert()
        .code(10)
        .get_output()
        .clone();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error"]["code"], "CONFIG_INVALID");
}
