//! `info`, `flash`, `reset`, and `run` without hardware: usage errors, guardrails,
//! dry runs, and `run --no-flash` over a replayed session. POGO_REPLAY stands in for the
//! device so nothing here can touch a connected board.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use assert_cmd::Command;
use serde_json::Value;

fn fixture(name: &str) -> String {
    format!(
        "{}/../../tests/fixtures/synthetic/{name}.jsonl",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// A fresh file per call: tests run in parallel, and rewriting a shared path lets one
/// test read another's half-written file.
fn scratch_file(name: &str, bytes: &[u8]) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("pogo-flash-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{}-{name}", N.fetch_add(1, Ordering::SeqCst)));
    std::fs::write(&path, bytes).unwrap();
    path
}

/// Enough of an ELF for pogo to classify it; it is never sent to a device here.
fn fake_elf() -> PathBuf {
    let mut bytes = b"\x7fELF".to_vec();
    bytes.resize(1024, 0);
    scratch_file("app.elf", &bytes)
}

fn pogo(args: &[&str], code: i32, schema: Option<&str>) -> Value {
    let out = Command::cargo_bin("pogo")
        .unwrap()
        .env_remove("POGO_FORMAT")
        .env_remove("POGO_CONFIG")
        .env("POGO_REPLAY", fixture("clean_boot"))
        .args(args)
        .args(["--wait-lock", "30s", "-f", "json"])
        .assert()
        .code(code)
        .get_output()
        .clone();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    if let Some(name) = schema {
        let path = format!("{}/../../schemas/{name}.json", env!("CARGO_MANIFEST_DIR"));
        let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let errors: Vec<String> = validator.iter_errors(&v).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{name}: {errors:#?}\n{v:#}");
    }
    v
}

#[test]
fn unreadable_or_unaddressed_images_are_usage_errors() {
    let v = pogo(&["flash", "/nonexistent/app.elf"], 2, None);
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("cannot read")
    );
    let bin = scratch_file("app.bin", &[0xe9, 0, 0, 0]);
    let v = pogo(&["flash", bin.to_str().unwrap()], 2, None);
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("--address")
    );
}

#[test]
fn raw_write_below_app_region_is_refused_with_exit_8() {
    let bin = scratch_file("boot.bin", &[0xe9, 0, 0, 0]);
    let path = bin.to_str().unwrap();
    let v = pogo(&["flash", path, "--address", "0x8000"], 8, None);
    assert_eq!(v["error"]["code"], "GUARDRAIL_REFUSED");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("--allow-bootloader-write")
    );
    let args = [
        "flash",
        path,
        "--address",
        "0x8000",
        "--allow-bootloader-write",
        "--dry-run",
    ];
    let v = pogo(&args, 0, Some("flash"));
    assert_eq!(v["data"]["plan"]["address"], "0x8000");
}

#[test]
fn erase_all_without_a_matching_serial_is_refused() {
    let elf = fake_elf();
    let args = [
        "flash",
        elf.to_str().unwrap(),
        "--erase-all",
        "--confirm",
        "AA:BB",
    ];
    let v = pogo(&args, 8, None);
    assert_eq!(v["error"]["code"], "GUARDRAIL_REFUSED");
}

#[test]
fn flash_dry_run_reports_a_plan() {
    let elf = fake_elf();
    let v = pogo(
        &["flash", elf.to_str().unwrap(), "--dry-run"],
        0,
        Some("flash"),
    );
    let plan = &v["data"]["plan"];
    assert_eq!(
        (plan["image"].as_str(), plan["bytes"].as_u64()),
        (Some("elf"), Some(1024))
    );
    assert_eq!(plan["verify"], true);
    assert_eq!(plan["reset_after"], true);
    assert!(v["data"].get("regions").is_none(), "nothing was flashed");
}

#[test]
fn replay_cannot_flash_or_read_info() {
    let elf = fake_elf();
    let v = pogo(&["flash", elf.to_str().unwrap()], 2, None);
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("POGO_REPLAY")
    );
    let v = pogo(&["info"], 2, None);
    assert_eq!(v["error"]["code"], "USAGE");
}

#[test]
fn reset_via_probe_is_not_implemented_and_dry_run_works() {
    let v = pogo(&["reset", "--via", "probe"], 1, None);
    assert_eq!(v["error"]["code"], "NOT_IMPLEMENTED");
    let v = pogo(
        &["reset", "--mode", "bootloader", "--dry-run"],
        0,
        Some("reset"),
    );
    assert_eq!(v["data"]["mode"], "bootloader");
    assert_eq!(v["data"]["dry_run"], true);
}

#[test]
fn run_no_flash_captures_after_reset() {
    let elf = fake_elf();
    let args = [
        "run",
        elf.to_str().unwrap(),
        "--no-flash",
        "--until",
        "READY",
        "--timeout",
        "300ms",
    ];
    let v = pogo(&args, 0, Some("run"));
    assert_eq!(v["data"]["flash"], Value::Null);
    assert_eq!(v["data"]["capture"]["outcome"], "matched");
    assert_eq!(v["data"]["capture"]["boot_count"], 1);
}

#[test]
fn run_dry_run_plans_the_flash_and_skips_capture() {
    let elf = fake_elf();
    let v = pogo(&["run", elf.to_str().unwrap(), "--dry-run"], 0, Some("run"));
    assert_eq!(v["data"]["plan"]["image"], "elf");
    assert_eq!(v["data"]["capture"], Value::Null);
}
