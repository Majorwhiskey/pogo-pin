//! Multi-process behavior of `serial capture`: device locks and signals.

#![allow(clippy::unwrap_used)]

use serde_json::Value;

fn fixture(name: &str) -> String {
    format!(
        "{}/../../tests/fixtures/synthetic/{name}.jsonl",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// A second capture on the same device fails with DEVICE_BUSY naming the holder, and
/// succeeds with --wait-lock once the holder finishes. Uses a private copy of the
/// fixture so no other test shares its lock.
#[test]
fn concurrent_capture_is_device_busy() {
    let dir = std::env::temp_dir().join(format!("pogo-busy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let own = dir.join("silent.jsonl");
    std::fs::copy(fixture("silent"), &own).unwrap();
    let pogo = |args: &[&str]| {
        let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin("pogo"));
        cmd.env_remove("POGO_FORMAT")
            .env_remove("POGO_CONFIG")
            .env("POGO_REPLAY", &own)
            .args(["serial", "capture", "-f", "json"])
            .args(args);
        cmd
    };

    let mut holder = pogo(&["--timeout", "3s", "--wait-lock", "10s"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    // The holder may start slowly; retry until it owns the lock. If a probe wins the race
    // instead, it completes (exit 0) and the holder, waiting on --wait-lock, goes next.
    let start = std::time::Instant::now();
    let busy = loop {
        let out = pogo(&["--timeout", "100ms"]).output().unwrap();
        if out.status.code() == Some(9) {
            break serde_json::from_slice::<Value>(&out.stdout).unwrap();
        }
        assert_eq!(out.status.code(), Some(0), "unexpected probe result");
        assert!(
            start.elapsed() < std::time::Duration::from_secs(8),
            "never saw DEVICE_BUSY"
        );
    };
    assert_eq!(busy["error"]["code"], "DEVICE_BUSY");
    assert_eq!(busy["error"]["details"]["pid"], holder.id());
    assert_eq!(busy["error"]["details"]["command"], "serial.capture");

    let waited = pogo(&["--timeout", "300ms", "--wait-lock", "10s"])
        .output()
        .unwrap();
    assert_eq!(waited.status.code(), Some(0));
    assert!(holder.wait().unwrap().success());
}

/// SIGINT mid-capture: partial envelope, INTERRUPTED, exit 130.
#[cfg(unix)]
#[test]
fn sigint_reports_partial_results_with_exit_130() {
    use std::io::{BufRead, BufReader};
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("pogo"))
        .env("POGO_REPLAY", fixture("clean_boot"))
        .args([
            "serial",
            "capture",
            "--timeout",
            "20s",
            "--wait-lock",
            "30s",
        ])
        .args(["-f", "jsonl"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut first = String::new();
    // The first streamed line means the handler is installed and the capture is running.
    stdout.read_line(&mut first).unwrap();
    assert!(first.contains(r#""type":"line""#), "{first}");
    let start = std::time::Instant::now();
    let killed = std::process::Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    let status = child.wait().unwrap();
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
    assert_eq!(status.code(), Some(130));
    let result: Value = stdout
        .lines()
        .map(|l| serde_json::from_str::<Value>(&l.unwrap()).unwrap())
        .find(|v| v["type"] == "result")
        .unwrap();
    assert_eq!(result["error"]["code"], "INTERRUPTED");
    assert_eq!(result["data"]["outcome"], "interrupted");
    assert_eq!(result["data"]["line_count"], 10);
}
