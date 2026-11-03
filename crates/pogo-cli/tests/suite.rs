//! `pogo test` over replayed sessions: pass, fail with JUnit, and bad suite files.
//! Replays deliver lines instantly, so `within` is tested in pogo-core instead.

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

/// A suite file in a private directory, so tests can run in parallel.
fn suite(name: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pogo-suite-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.toml"));
    std::fs::write(&path, body).unwrap();
    path
}

fn pogo(replay: &str, args: &[&str], code: i32) -> Value {
    let out = Command::cargo_bin("pogo")
        .unwrap()
        .env_remove("POGO_FORMAT")
        .env_remove("POGO_CONFIG")
        .env("POGO_REPLAY", fixture(replay))
        .args(["test"])
        .args(args)
        .args(["--wait-lock", "30s", "-f", "json"])
        .assert()
        .code(code)
        .get_output()
        .clone();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let path = format!("{}/../../schemas/test.json", env!("CARGO_MANIFEST_DIR"));
    let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let errors: Vec<String> = jsonschema::validator_for(&schema)
        .unwrap()
        .iter_errors(&v)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}\n{v:#}");
    v
}

const BOOTS: &str = r#"
[[test]]
name = "boots to READY"
steps = [{ reset = true, until = "READY", timeout = "500ms" }]
"#;

#[test]
fn passing_suite_exits_0() {
    let file = suite("pass", BOOTS);
    let v = pogo("clean_boot", &[file.to_str().unwrap()], 0);
    assert_eq!(
        (v["data"]["passed"].as_u64(), v["data"]["failed"].as_u64()),
        (Some(1), Some(0))
    );
    assert_eq!(v["data"]["tests"][0]["status"], "passed");
}

#[test]
fn crash_fails_the_test_with_exit_1_and_a_junit_report() {
    // This firmware prints READY and then panics, so wait for a line that never comes.
    let body = "[[test]]\nname = \"runs\"\nsteps = [{ reset = true, until = \"DONE\", timeout = \"500ms\" }]\n";
    let file = suite("crash", body);
    let junit = file.with_extension("xml");
    let args = [file.to_str().unwrap(), "--junit", junit.to_str().unwrap()];
    let v = pogo("rust_panic", &args, 1);
    assert_eq!(v["error"]["code"], "CHECK_FAILED");
    let f = &v["data"]["tests"][0]["failure"];
    assert_eq!(f["step"], 1);
    assert!(f["message"].as_str().unwrap().contains("panicked"), "{f:#}");
    assert!(!f["lines"].as_array().unwrap().is_empty());
    let xml = std::fs::read_to_string(&junit).unwrap();
    assert!(
        xml.contains(r#"<testsuites name="pogo" tests="1" failures="1""#),
        "{xml}"
    );
    assert!(xml.contains("<failure message=\"step 1: "), "{xml}");
    assert_eq!(v["data"]["junit_path"], junit.to_str().unwrap());
}

#[test]
fn bad_suite_files_are_usage_errors() {
    let v = pogo("clean_boot", &["/nonexistent/pogo-test.toml"], 2);
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("cannot read")
    );
    let file = suite(
        "bad",
        "[[test]]\nname = \"x\"\nsteps = [{ send = \"a\", until = \"b\" }]\n",
    );
    let v = pogo("clean_boot", &[file.to_str().unwrap()], 2);
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("`until` only goes with `reset`")
    );
}
