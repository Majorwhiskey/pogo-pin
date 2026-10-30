//! `pogo serial send` over recorded sessions (POGO_REPLAY).

#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
use serde_json::Value;

fn send(fixture: &str, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("pogo").unwrap();
    cmd.env_remove("POGO_FORMAT")
        .env_remove("POGO_CONFIG")
        .env("NO_COLOR", "1")
        .env(
            "POGO_REPLAY",
            format!(
                "{}/../../tests/fixtures/synthetic/{fixture}.jsonl",
                env!("CARGO_MANIFEST_DIR")
            ),
        )
        .args(["serial", "send", "--timeout", "300ms", "--wait-lock", "30s"])
        .args(args);
    cmd
}

fn json(fixture: &str, args: &[&str], code: i32) -> Value {
    let out = send(fixture, args)
        .args(["-f", "json"])
        .assert()
        .code(code)
        .get_output()
        .clone();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let schema_path = format!(
        "{}/../../schemas/serial.send.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path).unwrap()).unwrap();
    let errors: Vec<String> = jsonschema::validator_for(&schema)
        .unwrap()
        .iter_errors(&v)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    v
}

#[test]
fn expect_matches_the_reply() {
    let v = json("clean_boot", &["ping", "--expect", "READY"], 0);
    assert_eq!(v["data"]["bytes_written"], 5);
    assert_eq!(v["data"]["response"]["outcome"], "matched");
}

#[test]
fn without_expect_there_is_no_response() {
    let v = json("clean_boot", &["ping", "--line-ending", "crlf"], 0);
    assert_eq!(v["data"]["bytes_written"], 6);
    assert_eq!(v["data"]["response"], Value::Null);
}

#[test]
fn hex_and_no_line_ending() {
    let v = json(
        "clean_boot",
        &["--hex", "de ad be ef", "--line-ending", "none"],
        0,
    );
    assert_eq!(v["data"]["bytes_written"], 4);
}

#[test]
fn stdin_payload() {
    let out = send("clean_boot", &["-", "-f", "json"])
        .write_stdin("reboot")
        .assert()
        .code(0)
        .get_output()
        .clone();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["bytes_written"], 7);
}

#[test]
fn unanswered_expect_times_out_with_exit_4() {
    let v = json("silent", &["ping", "--expect", "pong"], 4);
    assert_eq!(v["error"]["code"], "TIMEOUT");
    assert_eq!(v["data"]["response"]["outcome"], "timeout");
}

#[test]
fn crash_in_reply_is_a_fault_with_exit_7() {
    let v = json("rust_panic", &["panic", "--expect", "DONE"], 7);
    assert_eq!(v["data"]["response"]["panic"]["location"]["line"], 47);
}

#[test]
fn disconnect_while_waiting_is_exit_6() {
    let v = json("disconnect", &["ping", "--expect", "pong"], 6);
    assert_eq!(v["error"]["code"], "DEVICE_IO");
}

#[test]
fn bad_hex_and_bad_pattern_are_usage_errors() {
    let v = json("clean_boot", &["--hex", "zz"], 2);
    assert_eq!(v["error"]["code"], "USAGE");
    let v = json("clean_boot", &["x", "--expect", "("], 2);
    assert!(v["error"]["message"].as_str().unwrap().contains("--expect"));
}
