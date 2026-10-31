//! Config settings take effect: flag > config > built-in default. POGO_REPLAY stands in
//! for the device; `--device target` names the config alias so its settings apply.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use serde_json::Value;

fn fixture(name: &str) -> String {
    format!(
        "{}/../../tests/fixtures/synthetic/{name}.jsonl",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn config(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pogo-cfg-apply-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.toml"));
    std::fs::write(&path, text).unwrap();
    path
}

const TARGET: &str = "[devices.target]\nkind = \"esp32\"\nmatch = { port = \"/dev/none\" }\n";

fn pogo(cfg: &PathBuf, replay: &str, args: &[&str]) -> (Option<i32>, Vec<u8>) {
    let out = Command::cargo_bin("pogo")
        .unwrap()
        .env_remove("POGO_FORMAT")
        .env("POGO_CONFIG", cfg)
        .env("POGO_REPLAY", fixture(replay))
        .args(args)
        .args(["--wait-lock", "30s"])
        .output()
        .unwrap();
    (out.status.code(), out.stdout)
}

#[test]
fn defaults_format_sets_the_output_format() {
    let cfg = config("format", "[defaults]\nformat = \"json\"\n");
    let (code, out) = pogo(&cfg, "clean_boot", &["config", "show"]);
    assert_eq!(code, Some(0));
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["command"], "config.show");
    let (_, human) = pogo(&cfg, "clean_boot", &["config", "show", "-f", "human"]);
    assert!(
        serde_json::from_slice::<Value>(&human).is_err(),
        "the flag wins"
    );
}

#[test]
fn defaults_timeout_applies_unless_the_flag_is_given() {
    let cfg = config("timeout", "[defaults]\ntimeout = \"150ms\"\n");
    let start = Instant::now();
    let (code, out) = pogo(
        &cfg,
        "silent",
        &["serial", "capture", "--until", "READY", "-f", "json"],
    );
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_eq!(code, Some(4));
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .ends_with("within 150ms")
    );

    let args = [
        "serial",
        "capture",
        "--until",
        "READY",
        "--timeout",
        "250ms",
        "-f",
        "json",
    ];
    let (_, out) = pogo(&cfg, "silent", &args);
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .ends_with("within 250ms")
    );
}

#[test]
fn device_parser_applies_unless_the_flag_is_given() {
    let cfg = config("parser", &format!("{TARGET}parser = \"raw\"\n"));
    let level = |extra: &[&str]| {
        let mut args = vec![
            "--device",
            "target",
            "serial",
            "capture",
            "--timeout",
            "300ms",
        ];
        args.extend_from_slice(extra);
        args.extend(["-f", "json"]);
        let (code, out) = pogo(&cfg, "clean_boot", &args);
        assert_eq!(code, Some(0));
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["data"]["device"]["alias"], "target");
        let line = v["data"]["lines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["text"].as_str().unwrap().starts_with("I (24) boot"))
            .unwrap()
            .clone();
        line["level"].clone()
    };
    assert_eq!(level(&[]), Value::Null, "config parser = raw");
    assert_eq!(level(&["--parser", "auto"]), "info", "the flag wins");
}

#[test]
fn probe_reset_in_config_is_not_implemented_yet() {
    let cfg = config("reset", &format!("{TARGET}reset = \"probe\"\n"));
    let args = [
        "--device", "target", "serial", "capture", "--reset", "-f", "json",
    ];
    let (code, out) = pogo(&cfg, "clean_boot", &args);
    assert_eq!(code, Some(1));
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["error"]["code"], "NOT_IMPLEMENTED");
    let (code, _) = pogo(
        &cfg,
        "clean_boot",
        &["--device", "target", "reset", "-f", "json"],
    );
    assert_eq!(code, Some(1));
}
