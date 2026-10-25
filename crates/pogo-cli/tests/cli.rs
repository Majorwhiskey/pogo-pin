//! CLI contract tests: exit codes, stream discipline, format flags, help text.

#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
use serde_json::Value;

fn pogo() -> Command {
    let mut cmd = Command::cargo_bin("pogo").unwrap();
    cmd.env_remove("POGO_FORMAT")
        .env_remove("POGO_CONFIG")
        .env("NO_COLOR", "1")
        .env("COLUMNS", "100");
    cmd
}

fn json(stdout: &[u8]) -> Value {
    let text = std::str::from_utf8(stdout).unwrap();
    assert_eq!(
        text.lines().count(),
        1,
        "expected one JSON line, got:\n{text}"
    );
    serde_json::from_str(text).unwrap()
}

#[test]
fn not_implemented_json_envelope_exits_1() {
    let out = pogo()
        .args(["info", "-f", "json"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert!(
        out.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out.stdout);
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["command"], "info");
    assert_eq!(v["ok"], false);
    assert_eq!(v["data"], Value::Null);
    assert_eq!(v["error"]["code"], "NOT_IMPLEMENTED");
    assert!(v["error"]["hint"].is_string());
    assert_eq!(v["warnings"], serde_json::json!([]));
}

#[test]
fn dotted_command_names() {
    let out = pogo()
        .args(["serial", "capture", "--format=json"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert_eq!(json(&out.stdout)["command"], "serial.capture");
}

#[test]
fn format_from_env() {
    let out = pogo()
        .env("POGO_FORMAT", "jsonl")
        .args(["probe", "info"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    let v = json(&out.stdout);
    assert_eq!(v["type"], "result");
    assert_eq!(v["command"], "probe.info");
}

#[test]
fn human_error_goes_to_stderr_only() {
    let out = pogo().arg("info").assert().code(1).get_output().clone();
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.starts_with("error[NOT_IMPLEMENTED]: not implemented yet: info"),
        "{stderr}"
    );
    assert!(stderr.contains("hint:"));
    assert!(!stderr.contains('\x1b'), "NO_COLOR must disable ANSI codes");
}

#[test]
fn usage_error_in_json_mode_is_an_envelope_with_exit_2() {
    let out = pogo()
        .args(["flash", "-f", "json"])
        .assert()
        .code(2)
        .get_output()
        .clone();
    let v = json(&out.stdout);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "USAGE");
    assert!(v["error"]["message"].as_str().unwrap().contains("<IMAGE>"));
}

#[test]
fn usage_error_in_human_mode_exits_2_on_stderr() {
    let out = pogo().args(["flash"]).assert().code(2).get_output().clone();
    assert!(out.stdout.is_empty());
    assert!(!out.stderr.is_empty());
}

#[test]
fn unknown_command_exits_2() {
    pogo().args(["frobnicate", "-f", "json"]).assert().code(2);
}

#[test]
fn erase_all_without_confirm_is_a_usage_error() {
    let out = pogo()
        .args(["flash", "app.elf", "--erase-all", "-f", "json"])
        .assert()
        .code(2)
        .get_output()
        .clone();
    assert_eq!(json(&out.stdout)["error"]["code"], "USAGE");
}

#[test]
fn help_and_version_exit_0() {
    pogo().arg("--help").assert().code(0);
    pogo()
        .arg("--version")
        .assert()
        .code(0)
        .stdout(predicates::str::starts_with("pogo "));
}

#[test]
fn completions_print_a_script() {
    pogo()
        .args(["completions", "zsh"])
        .assert()
        .code(0)
        .stdout(predicates::str::contains("#compdef pogo"));
}

const COMMANDS: &[&[&str]] = &[
    &[],
    &["devices"],
    &["doctor"],
    &["info"],
    &["flash"],
    &["reset"],
    &["serial"],
    &["serial", "capture"],
    &["serial", "send"],
    &["run"],
    &["i2c"],
    &["i2c", "scan"],
    &["i2c", "read"],
    &["i2c", "write"],
    &["gpio"],
    &["gpio", "get"],
    &["gpio", "set"],
    &["gpio", "pulse"],
    &["power"],
    &["power", "measure"],
    &["power", "on"],
    &["power", "off"],
    &["power", "cycle"],
    &["probe"],
    &["probe", "info"],
    &["probe", "selftest"],
    &["config"],
    &["config", "show"],
    &["config", "path"],
    &["config", "validate"],
    &["schema"],
    &["completions"],
    &["mcp"],
];

#[test]
fn help_snapshots() {
    for path in COMMANDS {
        let out = pogo()
            .args(*path)
            .arg("--help")
            .assert()
            .code(0)
            .get_output()
            .clone();
        let help = String::from_utf8(out.stdout).unwrap();
        let name = if path.is_empty() {
            "root".to_string()
        } else {
            path.join("_")
        };
        insta::assert_snapshot!(format!("help_{name}"), help);
    }
}

#[test]
fn leaf_help_has_examples_and_exit_codes() {
    for path in COMMANDS.iter().filter(|p| !p.is_empty()) {
        let out = pogo()
            .args(*path)
            .arg("--help")
            .assert()
            .code(0)
            .get_output()
            .clone();
        let help = String::from_utf8(out.stdout).unwrap();
        if help.contains("Commands:") {
            continue; // group command; its leaves are checked individually
        }
        assert!(help.contains("Examples:"), "{path:?} help lacks examples");
        assert!(
            help.contains("Exit codes:"),
            "{path:?} help lacks exit codes"
        );
        let examples = help
            .lines()
            .skip_while(|l| *l != "Examples:")
            .skip(1)
            .take_while(|l| l.starts_with("  "))
            .count();
        assert!(examples >= 2, "{path:?} help has fewer than 2 examples");
    }
}
