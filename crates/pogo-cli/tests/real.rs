//! Sessions recorded from real boards (tests/fixtures/real/), replayed through the CLI.

#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
use serde_json::Value;

fn real(name: &str, args: &[&str], code: i32) -> Value {
    let path = format!(
        "{}/../../tests/fixtures/real/{name}.jsonl",
        env!("CARGO_MANIFEST_DIR")
    );
    let out = Command::cargo_bin("pogo")
        .unwrap()
        .env("POGO_REPLAY", path)
        .args([
            "serial",
            "capture",
            "--timeout",
            "300ms",
            "--wait-lock",
            "30s",
        ])
        .args(args)
        .args(["-f", "json"])
        .assert()
        .code(code)
        .get_output()
        .clone();
    serde_json::from_slice(&out.stdout).unwrap()
}

/// pogo's test firmware (firmware/test-target) booting on an ESP32-WROOM-32 devkit.
#[test]
fn real_test_target_boot_reaches_ready() {
    let v = real(
        "esp32_test_target_ready",
        &["--reset", "--until", "READY"],
        0,
    );
    let lines = v["data"]["lines"].as_array().unwrap();
    let app = lines
        .iter()
        .find(|l| l["text"] == "INFO - test-target 0.1.0")
        .unwrap();
    assert_eq!(
        app["level"], "info",
        "esp-println's log format parses as rust-log"
    );
    assert_eq!(v["data"]["boot_count"], 1);
}

/// The test firmware's `panic` command: esp-backtrace prints a bare `Backtrace:` and
/// one address per line, which must all be collected.
#[test]
fn real_test_target_panic_reports_line_77() {
    let v = real("esp32_test_target_panic", &[], 7);
    let p = &v["data"]["panic"];
    assert_eq!(p["location"]["file"], "src/main.rs");
    assert_eq!(p["location"]["line"], 77);
    assert_eq!(p["location"]["column"], 25);
    assert_eq!(p["backtrace"].as_array().unwrap().len(), 6);
    assert_eq!(p["backtrace"][0], "0x400d28cd");
}

/// Recorded from an ESP32-WROOM-32 devkit (CH340 bridge): ROM boot text, then an app
/// whose output is not text at 115200 baud.
#[test]
fn real_esp32_devkit_boot_replays() {
    let real = format!(
        "{}/../../tests/fixtures/real/esp32_devkit_boot.jsonl",
        env!("CARGO_MANIFEST_DIR")
    );
    let out = Command::cargo_bin("pogo")
        .unwrap()
        .env("POGO_REPLAY", &real)
        .args([
            "serial",
            "capture",
            "--timeout",
            "300ms",
            "--wait-lock",
            "30s",
        ])
        .args(["--reset", "--until", "rst:0x", "-f", "json"])
        .assert()
        .code(0)
        .get_output()
        .clone();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["resets"][0]["reason"], "POWERON_RESET");
    assert_eq!(v["data"]["resets"][0]["code"], "0x1");
    assert_eq!(v["data"]["lines"][0]["text"], "ets Jul 29 2019 12:21:46");
}
