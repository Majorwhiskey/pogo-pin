//! `pogo setup claude` against a fake `claude` CLI that logs its arguments, so tests never
//! touch the real Claude Code configuration. Unix only: the fake is a shell script.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use assert_cmd::Command;
use serde_json::Value;

/// A directory holding only a fake `claude`; `registered` makes `mcp get` succeed.
fn fake_claude(name: &str, registered: bool) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("pogo-setup-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("calls.log");
    let get = if registered { 0 } else { 1 };
    let script = format!(
        "#!/bin/sh\necho \"$@\" >> '{}'\nif [ \"$1 $2\" = \"mcp get\" ]; then exit {get}; fi\nexit 0\n",
        log.display()
    );
    let claude = dir.join("claude");
    std::fs::write(&claude, script).unwrap();
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, log)
}

fn pogo(path: &PathBuf, args: &[&str], code: i32) -> Value {
    let out = Command::cargo_bin("pogo")
        .unwrap()
        .env_remove("POGO_FORMAT")
        .env("PATH", path)
        .args(["setup", "claude"])
        .args(args)
        .args(["-f", "json"])
        .assert()
        .code(code)
        .get_output()
        .clone();
    serde_json::from_slice(&out.stdout).unwrap()
}

fn calls(log: &PathBuf) -> Vec<String> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn registers_pogo_by_absolute_path() {
    let (dir, log) = fake_claude("add", false);
    let v = pogo(&dir, &[], 0);
    assert_eq!(v["data"]["status"], "added");
    let exe = assert_cmd::cargo::cargo_bin("pogo");
    let want = format!("mcp add --scope user pogo -- {} mcp", exe.display());
    assert_eq!(calls(&log), ["mcp get pogo".to_string(), want]);
}

#[test]
fn leaves_an_existing_registration_alone() {
    let (dir, log) = fake_claude("existing", true);
    let v = pogo(&dir, &["--scope", "project"], 0);
    assert_eq!(v["data"]["status"], "already_registered");
    assert_eq!(calls(&log), ["mcp get pogo"]);
}

#[test]
fn dry_run_and_plugin_runs_nothing_that_changes_config() {
    let (dir, log) = fake_claude("dry", false);
    let v = pogo(&dir, &["--plugin", "--dry-run"], 0);
    assert_eq!(v["data"]["status"], "planned");
    assert_eq!(
        v["data"]["command"],
        serde_json::json!([
            "claude",
            "plugin",
            "install",
            "pogo",
            "--marketplace",
            "Majorwhiskey/pogo-pin",
            "--scope",
            "user"
        ])
    );
    assert!(calls(&log).is_empty(), "{:?}", calls(&log));
}

#[test]
fn missing_claude_is_check_failed() {
    let empty = std::env::temp_dir().join(format!("pogo-setup-{}-empty", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    let v = pogo(&empty, &[], 1);
    assert_eq!(v["error"]["code"], "CHECK_FAILED");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not on PATH")
    );
}

#[test]
fn skips_a_claude_that_cannot_be_run() {
    let (dir, log) = fake_claude("shadowed", false);
    // An earlier PATH entry with a non-executable `claude`, as stale npm installs leave.
    let stale = dir.join("stale");
    std::fs::create_dir_all(&stale).unwrap();
    std::fs::write(stale.join("claude"), "not a program").unwrap();
    std::fs::set_permissions(stale.join("claude"), std::fs::Permissions::from_mode(0o644)).unwrap();
    let path = std::env::join_paths([&stale, &dir]).unwrap();
    let v = pogo(&PathBuf::from(path), &["--dry-run"], 0);
    assert_eq!(
        v["data"]["claude"],
        dir.join("claude").display().to_string()
    );
    assert_eq!(calls(&log), ["mcp get pogo"]);
}
