//! `power measure|on|off|cycle` against a replayed probe: statistics, CSV, guardrails,
//! and exit codes. Fixtures come from pogo-core's power tests.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use assert_cmd::Command;
use serde_json::Value;

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pogo-power-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A private copy per call: power history is kept per device, and the replay path is
/// the device, so runs must not share it.
fn fixture(name: &str) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let src = format!(
        "{}/../../tests/fixtures/synthetic/{name}.jsonl",
        env!("CARGO_MANIFEST_DIR")
    );
    let n = N.fetch_add(1, Ordering::SeqCst);
    let dst = scratch().join(format!("{name}-{n}.jsonl"));
    std::fs::copy(src, &dst).unwrap();
    dst.display().to_string()
}

fn config(toml: &str) -> PathBuf {
    let path = scratch().join(format!("pogo-{}.toml", toml.len()));
    std::fs::write(&path, toml).unwrap();
    path
}

fn json_with(replay: &str, cfg: &str, args: &[&str], code: i32) -> Value {
    let out = Command::cargo_bin("pogo")
        .unwrap()
        .env_remove("POGO_FORMAT")
        .env("POGO_CONFIG", config(cfg))
        .env("POGO_REPLAY", replay)
        .args(args)
        .args(["-f", "json"])
        .assert()
        .code(code)
        .get_output()
        .clone();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let path = format!(
        "{}/../../schemas/{}.json",
        env!("CARGO_MANIFEST_DIR"),
        v["command"].as_str().unwrap()
    );
    let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let errors: Vec<String> = jsonschema::validator_for(&schema)
        .unwrap()
        .iter_errors(&v)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{args:?}: {errors:#?}\n{v:#}");
    v
}

fn json(name: &str, args: &[&str], code: i32) -> Value {
    json_with(&fixture(name), "", args, code)
}

#[test]
fn measure_reports_stats_and_writes_csv() {
    let csv = scratch().join("samples.csv");
    let args = [
        "power",
        "measure",
        "--duration",
        "5ms",
        "--out",
        csv.to_str().unwrap(),
    ];
    let v = json("probe_power", &args, 0);
    let d = &v["data"];
    assert_eq!(
        (d["samples"].as_u64(), d["dropped"].as_u64()),
        (Some(4), Some(2))
    );
    assert_eq!(d["current_ma"]["max"], 312.0);
    assert_eq!(d["current_ma"]["p50"], 42.0);
    assert_eq!(d["bus_voltage_v"]["min"], 3.29);
    let text = std::fs::read_to_string(&csv).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[0],
        "index,t_ms,shunt_raw,bus_raw,current_ma,bus_voltage_v"
    );
    assert_eq!(lines[1], "0,0.000,1680,2640,42.0000,3.30000");
    assert_eq!(lines.len(), 5);
}

#[test]
fn measure_busy_and_bad_interval() {
    let v = json(
        "probe_power_busy",
        &["power", "measure", "--duration", "5ms"],
        9,
    );
    assert_eq!(v["error"]["code"], "DEVICE_BUSY");
    let v = json(
        "probe_power",
        &["power", "measure", "--interval", "10us"],
        2,
    );
    assert_eq!(v["error"]["code"], "USAGE");
}

#[test]
fn switch_and_cycle() {
    let v = json("probe_ok", &["power", "off"], 0);
    assert_eq!(v["data"]["on"], false);
    let v = json("probe_ok", &["power", "cycle", "--off-time", "200ms"], 0);
    assert_eq!(v["data"]["off_time_ms"], 200);
    let v = json("probe_silent", &["power", "on", "--dry-run"], 0);
    assert_eq!(v["data"]["dry_run"], true);
}

#[test]
fn off_time_and_rate_guardrails_exit_8() {
    let v = json("probe_silent", &["power", "cycle", "--off-time", "50ms"], 8);
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("min_power_off_time")
    );

    // One probe, one cycle allowed per minute: the second is refused.
    let replay = fixture("probe_ok");
    let cfg = "[guardrails]\nmax_power_cycles_per_minute = 1\n";
    json_with(&replay, cfg, &["power", "cycle", "--off-time", "200ms"], 0);
    let v = json_with(&replay, cfg, &["power", "cycle", "--off-time", "200ms"], 8);
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("1 power cycles")
    );

    // Off then straight back on is too short an off time.
    let replay = fixture("probe_ok");
    json_with(&replay, "", &["power", "off"], 0);
    let v = json_with(&replay, "", &["power", "on"], 8);
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("minimum off time")
    );
}
