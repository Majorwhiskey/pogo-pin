//! `pogo serial capture` over recorded sessions (POGO_REPLAY): outcomes, exit codes,
//! streaming, and schema conformance.

#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
use serde_json::Value;

fn fixture(name: &str) -> String {
    format!(
        "{}/../../tests/fixtures/synthetic/{name}.jsonl",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn capture(name: &str, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("pogo").unwrap();
    cmd.env_remove("POGO_FORMAT")
        .env_remove("POGO_CONFIG")
        .env("NO_COLOR", "1")
        .env("POGO_REPLAY", fixture(name))
        // Tests run in parallel and share fixtures, so they take turns on each lock.
        .args([
            "serial",
            "capture",
            "--timeout",
            "300ms",
            "--wait-lock",
            "30s",
        ])
        .args(args);
    cmd
}

fn json(name: &str, args: &[&str], code: i32) -> Value {
    let out = capture(name, args)
        .args(["-f", "json"])
        .assert()
        .code(code)
        .get_output()
        .clone();
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let schema_path = format!(
        "{}/../../schemas/serial.capture.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path).unwrap()).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator.iter_errors(&v).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{name}: {errors:#?}");
    v
}

#[test]
fn clean_boot_matches_ready() {
    let v = json("clean_boot", &["--reset", "--until", "READY"], 0);
    let d = &v["data"];
    assert_eq!(d["outcome"], "matched");
    assert_eq!(d["matched"]["pattern"], "READY");
    assert_eq!(d["boot_count"], 1);
    assert_eq!(d["resets"][0]["reason"], "POWERON");
    assert_eq!(
        d["lines"][6]["text"],
        "I (24) boot: ESP-IDF v5.1 2nd stage bootloader"
    );
    assert_eq!(d["lines"][6]["level"], "info");
    assert!(d["device"]["port"].as_str().unwrap().starts_with("replay:"));
}

#[test]
fn silence_times_out_with_exit_4() {
    let v = json("silent", &["--until", "READY"], 4);
    assert_eq!(v["error"]["code"], "TIMEOUT");
    assert_eq!(v["data"]["outcome"], "timeout");
    assert_eq!(v["data"]["line_count"], 0);
}

#[test]
fn no_until_completes_with_exit_0() {
    let v = json("clean_boot", &[], 0);
    assert_eq!(v["data"]["outcome"], "completed");
}

#[test]
fn rust_panic_reports_location_with_exit_7() {
    let v = json("rust_panic", &["--reset", "--until", "DONE"], 7);
    assert_eq!(v["error"]["code"], "TARGET_FAULT");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("src/bin/main.rs:47:5"), "{msg}");
    let p = &v["data"]["panic"];
    assert_eq!(p["location"]["file"], "src/bin/main.rs");
    assert_eq!(p["location"]["line"], 47);
    assert_eq!(p["message"], "called `Option::unwrap()` on a `None` value");
    assert_eq!(v["data"]["fault"]["kind"], "panic");
    assert_eq!(
        v["data"]["boot_count"], 1,
        "the reboot after the panic ends the block"
    );
}

#[test]
fn guru_meditation_reports_backtrace_with_exit_7() {
    let v = json("guru_meditation", &["--until", "READY"], 7);
    assert_eq!(
        v["data"]["panic"]["backtrace"],
        serde_json::json!(["0x4200a1b2", "0x4200a19c", "0x40385a2e"])
    );
}

#[test]
fn boot_loop_after_reset_is_unexpected_reboot() {
    let v = json("boot_loop", &["--reset", "--until", "READY"], 7);
    assert_eq!(v["data"]["fault"]["kind"], "unexpected_reboot");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("RTC_SW_CPU_RST")
    );
}

#[test]
fn brownout_matches_default_fail_pattern() {
    let v = json("brownout", &["--until", "READY"], 7);
    assert_eq!(v["data"]["fault"]["kind"], "fail_on");
    assert_eq!(
        v["data"]["fault"]["pattern"],
        "Brownout detector was triggered"
    );
    let v = json("brownout", &["--no-default-fail-patterns"], 0);
    assert_eq!(v["data"]["outcome"], "completed");
}

#[test]
fn disconnect_mid_capture_is_eof_with_exit_6() {
    let v = json("disconnect", &["--until", "READY"], 6);
    assert_eq!(v["error"]["code"], "DEVICE_IO");
    assert_eq!(v["data"]["outcome"], "eof");
    let lines = v["data"]["lines"].as_array().unwrap();
    assert_eq!(lines.last().unwrap()["text"], "load:0x3fcd5820,len:0x17");
}

#[test]
fn custom_fail_on_and_bad_pattern() {
    let v = json(
        "clean_boot",
        &["--fail-on", "wifi connected", "--until", "READY"],
        7,
    );
    assert_eq!(v["data"]["fault"]["pattern"], "wifi connected");
    let out = capture("clean_boot", &["--until", "(", "-f", "json"])
        .assert()
        .code(2)
        .get_output()
        .clone();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error"]["code"], "USAGE");
}

#[test]
fn jsonl_streams_lines_then_result() {
    let out = capture("clean_boot", &["--until", "READY", "-f", "jsonl"])
        .assert()
        .code(0)
        .get_output()
        .clone();
    let events: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let (last, lines) = events.split_last().unwrap();
    assert_eq!(last["type"], "result");
    assert_eq!(lines.len(), 10);
    assert!(lines.iter().all(|e| e["type"] == "line"));
    assert_eq!(lines[9]["text"], "READY");
}

#[test]
fn human_streams_lines_then_a_summary() {
    let out = capture("brownout", &["--until", "READY"])
        .assert()
        .code(7)
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout)
        .unwrap()
        .replace(&fixture("brownout"), "[fixture]");
    insta::assert_snapshot!("capture_human_stdout", stdout);
    insta::assert_snapshot!(
        "capture_human_stderr",
        String::from_utf8(out.stderr).unwrap()
    );
}

#[test]
fn record_writes_a_replayable_session() {
    let dir = std::env::temp_dir().join(format!("pogo-record-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("again.jsonl");
    let first = json(
        "clean_boot",
        &["--until", "READY", "--record", path.to_str().unwrap()],
        0,
    );
    let mut cmd = Command::cargo_bin("pogo").unwrap();
    let out = cmd
        .env("POGO_REPLAY", &path)
        .args([
            "serial",
            "capture",
            "--timeout",
            "300ms",
            "--until",
            "READY",
            "-f",
            "json",
        ])
        .assert()
        .code(0)
        .get_output()
        .clone();
    let second: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(first["data"]["lines"], second["data"]["lines"]);
}

/// A capture bigger than the ~200 KB soft cap loses middle lines with a warning.
#[test]
fn oversized_output_is_capped_with_a_warning() {
    let dir = std::env::temp_dir().join(format!("pogo-cap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("flood.jsonl");
    let mut text = String::from(r#"{"type":"header","format":1,"pogo_version":"0.1.0"}"#);
    text.push('\n');
    let body: String = (0..4000)
        .map(|i| format!("{i:05} {}\\n", "x".repeat(140)))
        .collect();
    text.push_str(&format!(r#"{{"type":"rx","t_ms":0,"text":"{body}"}}"#));
    text.push('\n');
    std::fs::write(&path, text).unwrap();

    let out = Command::cargo_bin("pogo")
        .unwrap()
        .env("POGO_REPLAY", &path)
        .args([
            "serial",
            "capture",
            "--timeout",
            "300ms",
            "--max-lines",
            "100000",
        ])
        .args(["-f", "json"])
        .assert()
        .code(0)
        .get_output()
        .clone();
    assert!(out.stdout.len() < 210 * 1024, "{} bytes", out.stdout.len());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["warnings"][0]["code"], "OUTPUT_TRUNCATED");
    let d = &v["data"];
    assert_eq!(d["line_count"], 4000);
    let kept = d["lines"].as_array().unwrap().len() as u64;
    assert_eq!(kept + d["omitted_lines"].as_u64().unwrap(), 4000);
    assert_eq!(
        d["lines"][0]["text"].as_str().unwrap().get(..5),
        Some("00000")
    );
}
