//! Classic ESP32 (Xtensa) fault output over replayed sessions: each built-in fail
//! pattern ends the capture with exit 7, and panic blocks carry their backtrace.

#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
use serde_json::{Value, json};

fn capture(name: &str, code: i32) -> Value {
    let fixture = format!(
        "{}/../../tests/fixtures/synthetic/{name}.jsonl",
        env!("CARGO_MANIFEST_DIR")
    );
    let out = Command::cargo_bin("pogo")
        .unwrap()
        .env_remove("POGO_FORMAT")
        .env_remove("POGO_CONFIG")
        .env("POGO_REPLAY", fixture)
        .args([
            "serial",
            "capture",
            "--until",
            "READY",
            "--timeout",
            "300ms",
        ])
        .args(["--wait-lock", "30s", "-f", "json"])
        .assert()
        .code(code)
        .get_output()
        .clone();
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn xtensa_guru_meditation_keeps_pcs_and_message() {
    let v = capture("esp32_guru_meditation", 7);
    let p = &v["data"]["panic"];
    assert_eq!(
        p["backtrace"],
        json!(["0x400d1231", "0x400d5675", "0x40088a41"])
    );
    assert!(p["message"].as_str().unwrap().contains("LoadProhibited"));
    assert_eq!(v["data"]["fault"]["pattern"], "Guru Meditation Error");
}

#[test]
fn abort_reports_its_line_and_drops_the_corrupted_marker() {
    let v = capture("esp32_abort", 7);
    let p = &v["data"]["panic"];
    assert_eq!(
        p["backtrace"],
        json!(["0x40081a3e", "0x40087b4d", "0x400d2f3b"])
    );
    assert_eq!(
        p["message"],
        "abort() was called at PC 0x400d2f3b on core 0"
    );
    assert_eq!(
        v["data"]["boot_count"], 1,
        "the reboot is not a second boot"
    );
}

#[test]
fn task_watchdog_is_a_fault_with_the_log_level_kept() {
    let v = capture("task_watchdog", 7);
    assert_eq!(v["error"]["code"], "TARGET_FAULT");
    assert_eq!(v["data"]["fault"]["pattern"], "Task watchdog got triggered");
    let i = v["data"]["fault"]["line_index"].as_u64().unwrap() as usize;
    let line = &v["data"]["lines"][i];
    assert_eq!(
        (line["level"].as_str(), line["tag"].as_str()),
        (Some("error"), Some("task_wdt"))
    );
}

#[test]
fn stack_overflow_is_a_fault() {
    let v = capture("stack_overflow", 7);
    assert_eq!(v["data"]["fault"]["pattern"], "stack overflow");
    assert_eq!(v["data"]["outcome"], "fault");
}
