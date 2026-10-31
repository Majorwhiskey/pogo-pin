//! Hardware-in-the-loop tests (docs/testing.md §12). They flash, reset, and talk to a real
//! board, so they run only when asked:
//!
//!     POGO_HIL=1 cargo test -p pogo-cli --test hil -- --test-threads=1
//!
//! Needs one ESP32 connected (or `POGO_HIL_DEVICE=<selector>`) and the test firmware built
//! (`cargo build --release` in firmware/test-target). Without `POGO_HIL=1` every test
//! returns at once, so `cargo test` and CI never touch hardware.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn enabled() -> bool {
    std::env::var_os("POGO_HIL").is_some_and(|v| v == "1")
}

fn elf() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../firmware/test-target/target/xtensa-esp32-none-elf/release/test-target");
    assert!(
        path.is_file(),
        "build the test firmware first: cd firmware/test-target && cargo build --release"
    );
    path.display().to_string()
}

/// Run pogo against the board; returns the exit code and the JSON envelope.
fn pogo(args: &[&str]) -> (i32, Value) {
    let device = std::env::var("POGO_HIL_DEVICE").unwrap_or_else(|_| "auto".into());
    let out = Command::new(assert_cmd::cargo::cargo_bin("pogo"))
        .env_remove("POGO_FORMAT")
        .env_remove("POGO_REPLAY")
        .args(["--device", &device, "--wait-lock", "30s"])
        .args(args)
        .args(["-f", "json"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{args:?}: {e}\n{}", String::from_utf8_lossy(&out.stdout)));
    (out.status.code().unwrap(), v)
}

/// Make sure the board runs the test firmware and is at READY, flashing it if needed.
fn ready() {
    let (code, _) = pogo(&[
        "run",
        &elf(),
        "--no-flash",
        "--until",
        "^READY$",
        "--timeout",
        "8s",
    ]);
    if code != 0 {
        let (code, v) = pogo(&["run", &elf(), "--until", "^READY$", "--timeout", "15s"]);
        assert_eq!(code, 0, "could not flash the test firmware: {v:#}");
    }
}

#[test]
fn run_flashes_and_reaches_ready() {
    if !enabled() {
        return;
    }
    let (code, v) = pogo(&["run", &elf(), "--until", "^READY$", "--timeout", "15s"]);
    assert_eq!(code, 0, "{v:#}");
    let d = &v["data"];
    assert!(d["flash"]["chip"].as_str().unwrap().starts_with("esp32"));
    assert_eq!(d["capture"]["outcome"], "matched");
    assert_eq!(d["capture"]["boot_count"], 1);
    let lines = d["capture"]["lines"].as_array().unwrap();
    assert_eq!(
        lines.last().unwrap()["text"],
        "READY",
        "capture stops at the match"
    );
}

#[test]
fn ping_gets_pong() {
    if !enabled() {
        return;
    }
    ready();
    let (code, v) = pogo(&[
        "serial",
        "send",
        "ping",
        "--expect",
        "^pong$",
        "--timeout",
        "3s",
    ]);
    assert_eq!(code, 0, "{v:#}");
}

#[test]
fn panic_is_reported_at_its_line_and_symbolized() {
    if !enabled() {
        return;
    }
    ready();
    let (_, devices) = pogo(&["devices"]);
    let port = devices["data"]["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["kind"] == "esp32_target")
        .unwrap()["port"]
        .as_str()
        .unwrap()
        .to_string();
    // A config naming the board with the firmware ELF, so serial send can symbolize.
    let dir = std::env::temp_dir().join(format!("pogo-hil-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("pogo.toml");
    let toml = format!(
        "[devices.hil]\nkind = \"esp32\"\nmatch = {{ port = \"{port}\" }}\nelf = \"{}\"\n",
        elf().replace('\\', "/")
    );
    std::fs::write(&cfg, toml).unwrap();
    let out = Command::new(assert_cmd::cargo::cargo_bin("pogo"))
        .env("POGO_CONFIG", &cfg)
        .args([
            "--device", "hil", "serial", "send", "panic", "--expect", "NEVER",
        ])
        .args(["--timeout", "4s", "-f", "json"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(7), "{v:#}");
    let p = &v["data"]["response"]["panic"];
    assert_eq!(p["location"]["file"], "src/main.rs");
    assert_eq!(p["location"]["line"], 77);
    let frames = p["frames"].as_array().unwrap();
    assert!(
        frames.iter().any(|f| f["function"]
            .as_str()
            .is_some_and(|n| n.ends_with("trigger_panic"))),
        "{frames:#?}"
    );
}

#[test]
fn reset_modes_stop_and_restart_the_app() {
    if !enabled() {
        return;
    }
    ready();
    let (code, v) = pogo(&["reset", "--mode", "bootloader"]);
    assert_eq!(code, 0, "{v:#}");
    let (code, _) = pogo(&[
        "serial",
        "send",
        "ping",
        "--expect",
        "^pong$",
        "--timeout",
        "1500ms",
    ]);
    assert_eq!(code, 4, "the app must not answer from the ROM bootloader");
    let (code, _) = pogo(&["reset"]);
    assert_eq!(code, 0);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let (code, _) = pogo(&[
        "serial",
        "send",
        "ping",
        "--expect",
        "^pong$",
        "--timeout",
        "3s",
    ]);
    assert_eq!(code, 0);
}

#[test]
fn opening_the_port_does_not_reset_the_board() {
    if !enabled() {
        return;
    }
    ready();
    for _ in 0..3 {
        let (code, v) = pogo(&["serial", "capture", "--timeout", "800ms"]);
        assert_eq!(code, 0);
        assert_eq!(v["data"]["boot_count"], 0, "{v:#}");
    }
}

#[test]
fn info_and_deep_doctor() {
    if !enabled() {
        return;
    }
    let (code, v) = pogo(&["info"]);
    assert_eq!(code, 0, "{v:#}");
    assert!(v["data"]["chip"].as_str().unwrap().starts_with("esp32"));
    assert!(v["data"]["flash_size_bytes"].as_u64().unwrap() > 0);
    let (code, v) = pogo(&["doctor", "--deep"]);
    assert_eq!(code, 0, "{v:#}");
    let checks = v["data"]["checks"].as_array().unwrap();
    assert!(
        checks
            .iter()
            .any(|c| c["id"] == "open" && c["status"] == "pass")
    );
}
