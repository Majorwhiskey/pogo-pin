//! `pogo doctor`: envelope shape and exit codes. Which boards are attached varies by
//! machine, so only the config failure path asserts a specific outcome.

#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
use serde_json::Value;

fn doctor(args: &[&str]) -> (Option<i32>, Value) {
    let out = Command::cargo_bin("pogo")
        .unwrap()
        .env_remove("POGO_CONFIG")
        .env_remove("POGO_FORMAT")
        .args(["doctor", "-f", "json"])
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code(),
        serde_json::from_slice(&out.stdout).unwrap(),
    )
}

#[test]
fn checks_validate_against_the_schema() {
    let (code, v) = doctor(&[]);
    let schema_path = format!("{}/../../schemas/doctor.json", env!("CARGO_MANIFEST_DIR"));
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path).unwrap()).unwrap();
    let errors: Vec<String> = jsonschema::validator_for(&schema)
        .unwrap()
        .iter_errors(&v)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    let checks = v["data"]["checks"].as_array().unwrap();
    let ids: Vec<&str> = checks.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert!(ids.starts_with(&["config", "devices"]), "{ids:?}");
    assert!(ids.contains(&"probe"));
    let any_fail = checks.iter().any(|c| c["status"] == "fail");
    match code {
        Some(0) => assert!(!any_fail && v["ok"] == true),
        Some(1) => {
            assert!(any_fail);
            assert_eq!(v["error"]["code"], "CHECK_FAILED");
        }
        other => panic!("unexpected exit {other:?}: {v}"),
    }
}

#[test]
fn invalid_config_fails_the_config_check_with_exit_10() {
    let dir = std::env::temp_dir().join(format!("pogo-doctor-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.toml");
    std::fs::write(&bad, "[defaults]\nbuad = 1\n").unwrap();
    let (code, v) = doctor(&["--config", bad.to_str().unwrap()]);
    assert_eq!(code, Some(10));
    assert_eq!(v["error"]["code"], "CONFIG_INVALID");
    let config = &v["data"]["checks"][0];
    assert_eq!(
        (config["id"].as_str(), config["status"].as_str()),
        (Some("config"), Some("fail"))
    );
    assert!(config["hint"].as_str().unwrap().contains("config validate"));
}
