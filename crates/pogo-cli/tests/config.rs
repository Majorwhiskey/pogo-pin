//! `pogo config show|path|validate`: data shapes, exit codes, human output.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use assert_cmd::Command;
use serde_json::{Value, json};

/// A fresh directory with an empty home and a project dir. Canonicalized on Unix so it
/// matches the binary's current directory (macOS temp lives under a /private symlink);
/// not on Windows, where canonical paths gain a `\\?\` prefix the binary never reports.
fn bench(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pogo-cli-{name}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(dir.join("home")).unwrap();
    std::fs::create_dir_all(dir.join("proj")).unwrap();
    if cfg!(windows) {
        dir
    } else {
        dir.canonicalize().unwrap()
    }
}

fn pogo(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("pogo").unwrap();
    cmd.env_remove("POGO_FORMAT")
        .env_remove("POGO_CONFIG")
        .env_remove("XDG_CONFIG_HOME")
        .env("HOME", dir.join("home"))
        .env("NO_COLOR", "1")
        .current_dir(dir.join("proj"));
    cmd
}

fn run_json(cmd: &mut Command, code: i32) -> Value {
    let out = cmd.assert().code(code).get_output().clone();
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

/// Returns the path as the binary will print it: native separators throughout.
fn write(path: &Path, text: &str) -> String {
    std::fs::write(path, text).unwrap();
    path.components().collect::<PathBuf>().display().to_string()
}

#[test]
fn path_lists_candidates_in_merge_order() {
    let dir = bench("path");
    let project = write(&dir.join("proj/pogo.toml"), "schema = 1\n");
    let v = run_json(
        pogo(&dir).args([
            "config",
            "path",
            "-f",
            "json",
            "--config",
            "../missing.toml",
        ]),
        0,
    );
    assert_eq!(v["command"], "config.path");
    let files = v["data"]["files"].as_array().unwrap();
    let kinds: Vec<_> = files.iter().map(|f| f["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["user", "project", "explicit"]);
    assert_eq!(
        files[1],
        json!({ "path": project, "kind": "project", "exists": true })
    );
    assert_eq!(files[2]["exists"], false);
}

#[test]
fn show_merges_project_and_explicit_with_sources() {
    let dir = bench("show");
    let project = write(
        &dir.join("proj/pogo.toml"),
        "[defaults]\nbaud = 115200\nformat = \"json\"\n",
    );
    let explicit = write(&dir.join("bench.toml"), "[defaults]\nbaud = 921600\n");
    let v = run_json(
        pogo(&dir)
            .env("POGO_CONFIG", &explicit)
            .args(["config", "show", "-f", "json"]),
        0,
    );
    assert_eq!(
        v["data"],
        json!({
            "config": { "defaults": { "baud": 921600, "format": "json" } },
            "sources": [
                { "key": "defaults.baud", "source": explicit },
                { "key": "defaults.format", "source": project },
            ]
        })
    );
}

#[test]
fn validate_ok_lists_loaded_files() {
    let dir = bench("valid");
    let project = write(&dir.join("proj/pogo.toml"), "[defaults]\nbaud = 9600\n");
    let v = run_json(pogo(&dir).args(["config", "validate", "-f", "json"]), 0);
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"], json!({ "files": [project], "valid": true }));
}

#[test]
fn invalid_config_exits_10() {
    let dir = bench("invalid");
    write(&dir.join("proj/pogo.toml"), "[defaults]\nbuad = 9600\n");
    for sub in ["validate", "show"] {
        let v = run_json(pogo(&dir).args(["config", sub, "-f", "json"]), 10);
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"]["code"], "CONFIG_INVALID");
        let msg = v["error"]["message"].as_str().unwrap();
        assert!(msg.contains("pogo.toml") && msg.contains("line 2"), "{msg}");
    }
}

#[test]
fn missing_explicit_config_exits_10() {
    let dir = bench("missing");
    let v = run_json(
        pogo(&dir).args(["config", "validate", "-f", "json", "--config", "nope.toml"]),
        10,
    );
    assert_eq!(v["error"]["code"], "CONFIG_INVALID");
}

fn human(dir: &Path, args: &[&str], code: i32) -> (String, String) {
    let out = pogo(dir)
        .args(args)
        .assert()
        .code(code)
        .get_output()
        .clone();
    let tmp = dir.display().to_string();
    let clean = |b: &[u8]| {
        String::from_utf8_lossy(b)
            .replace(&tmp, "[bench]")
            .replace('\\', "/")
    };
    (clean(&out.stdout), clean(&out.stderr))
}

#[test]
fn human_show_and_validate() {
    let dir = bench("human");
    write(
        &dir.join("proj/pogo.toml"),
        "[devices.target]\nkind = \"esp32\"\nmatch = { vid = 0x303a }\n",
    );
    let (show, err) = human(&dir, &["config", "show"], 0);
    assert!(err.is_empty());
    insta::assert_snapshot!("human_config_show", show);
    let (valid, err) = human(&dir, &["config", "validate"], 0);
    assert!(err.is_empty());
    insta::assert_snapshot!("human_config_validate", valid);

    write(
        &dir.join("proj/pogo.toml"),
        "[devices.target]\nkind = \"esp32\"\n",
    );
    let (out, err) = human(&dir, &["config", "validate"], 10);
    assert!(out.is_empty());
    insta::assert_snapshot!("human_config_invalid", err);
}

#[test]
fn human_no_files() {
    let dir = bench("nofiles");
    let (out, _) = human(&dir, &["config", "show"], 0);
    assert_eq!(out, "No config files found; built-in defaults apply.\n");
}
