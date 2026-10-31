//! `pogo schema`: raw schema output, exit codes, `--all --out`, and output conformance.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;

use assert_cmd::Command;
use serde_json::Value;

fn pogo() -> Command {
    let mut cmd = Command::cargo_bin("pogo").unwrap();
    cmd.env_remove("POGO_FORMAT")
        .env_remove("POGO_CONFIG")
        .env("NO_COLOR", "1");
    cmd
}

fn stdout_json(args: &[&str], code: i32) -> Value {
    let out = pogo().args(args).assert().code(code).get_output().clone();
    serde_json::from_slice(&out.stdout).unwrap()
}

fn keys(v: &Value) -> BTreeSet<String> {
    v.as_object().unwrap().keys().cloned().collect()
}

fn committed(name: &str) -> Value {
    let path = format!("{}/../../schemas/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn single_schema_is_a_bare_document() {
    let s = stdout_json(&["schema", "devices", "-f", "json"], 0);
    assert_eq!(s["title"], "pogo devices envelope");
    assert!(s.get("schema_version").is_none(), "must not be wrapped");
    assert_eq!(s, committed("devices"));
}

#[test]
fn unknown_command_exits_2_and_unimplemented_exits_1() {
    let v = stdout_json(&["schema", "nope", "-f", "json"], 2);
    assert_eq!(v["error"]["code"], "USAGE");
    assert!(
        v["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("serial.capture")
    );
    let v = stdout_json(&["schema", "i2c.scan", "-f", "json"], 1);
    assert_eq!(v["error"]["code"], "NOT_IMPLEMENTED");
}

#[test]
fn all_to_stdout_and_to_dir_match_committed() {
    let all = stdout_json(&["schema", "--all"], 0);
    let names = keys(&all);
    assert_eq!(
        names,
        [
            "config.path",
            "config.show",
            "config.validate",
            "devices",
            "doctor",
            "flash",
            "info",
            "reset",
            "run",
            "serial.capture",
            "serial.send",
        ]
        .map(String::from)
        .into()
    );

    let dir = std::env::temp_dir().join(format!("pogo-schemas-{}", std::process::id()));
    let v = stdout_json(
        &[
            "schema",
            "--all",
            "--out",
            dir.to_str().unwrap(),
            "-f",
            "json",
        ],
        0,
    );
    assert_eq!(v["data"]["files"].as_array().unwrap().len(), names.len());
    for name in &names {
        let text = std::fs::read_to_string(dir.join(format!("{name}.json"))).unwrap();
        let committed_text = std::fs::read_to_string(format!(
            "{}/../../schemas/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        assert_eq!(
            text, committed_text,
            "{name}: run `pogo schema --all --out schemas/`"
        );
        assert_eq!(all[name], committed(name));
    }
}

fn assert_valid(schema_name: &str, instance: &Value) {
    let validator = jsonschema::validator_for(&committed(schema_name)).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "{schema_name}: {errors:#?}\n{instance:#}"
    );
}

#[test]
fn real_output_validates_against_schemas() {
    let example = format!("{}/../../pogo.example.toml", env!("CARGO_MANIFEST_DIR"));
    let cases: [(&str, &[&str], i32); 5] = [
        ("devices", &["devices", "--all"], 0),
        ("config.show", &["config", "show", "--config", &example], 0),
        ("config.path", &["config", "path"], 0),
        (
            "config.validate",
            &["config", "validate", "--config", &example],
            0,
        ),
        (
            "config.validate",
            &["config", "validate", "--config", "/nonexistent.toml"],
            10,
        ),
    ];
    for (schema, args, code) in cases {
        let mut args = args.to_vec();
        args.extend(["-f", "json"]);
        assert_valid(schema, &stdout_json(&args, code));
    }
}

#[test]
fn schema_rejects_a_bad_device_kind() {
    let mut v = stdout_json(&["devices", "-f", "json"], 0);
    v["data"]["devices"] = serde_json::json!([{
        "alias": null, "kind": "bogus", "port": "/dev/x", "usb": null,
        "chip_hint": null, "locked_by": null
    }]);
    let validator = jsonschema::validator_for(&committed("devices")).unwrap();
    assert!(!validator.is_valid(&v));
    v["data"]["devices"][0]["kind"] = "esp32_target".into();
    assert!(validator.is_valid(&v));
}
