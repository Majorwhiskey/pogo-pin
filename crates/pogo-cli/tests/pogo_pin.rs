//! `pogo-pin` forwards to `pogo`, and the menu never appears without a terminal.

use std::process::Command;

fn bin(name: &str) -> Command {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin(name));
    cmd.env_remove("POGO_FORMAT").env("NO_COLOR", "1");
    cmd
}

#[test]
fn pogo_pin_runs_pogo_with_the_same_arguments() {
    let via_pin = bin("pogo-pin").args(["--version"]).output().unwrap();
    let direct = bin("pogo").args(["--version"]).output().unwrap();
    assert_eq!(via_pin.status.code(), Some(0));
    assert_eq!(via_pin.stdout, direct.stdout);

    let bad = bin("pogo-pin").args(["no-such-command"]).output().unwrap();
    assert_eq!(bad.status.code(), Some(2), "exit codes pass through");
}

#[test]
fn bare_invocation_without_a_terminal_is_a_usage_error_not_the_menu() {
    for name in ["pogo", "pogo-pin"] {
        let out = bin(name).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{name}");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(!stdout.contains("Select an option"), "{name}: {stdout}");
    }
}
