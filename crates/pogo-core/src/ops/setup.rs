//! `pogo setup claude`: connect pogo to Claude Code in one step, by registering its MCP
//! server (or installing the pogo plugin) with the `claude` CLI.

use std::path::{Path, PathBuf};
use std::process::Command;

use schemars::JsonSchema;
use serde::Serialize;

use super::OpResult;
use crate::envelope::ErrorBody;
use crate::error::ErrorCode;

/// Where the plugin is published: this repository's marketplace.
pub const MARKETPLACE: &str = "Majorwhiskey/pogo-pin";
const SERVER: &str = "pogo";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// This project, for this user only.
    Local,
    /// This project, shared through the repository.
    Project,
    /// Every project for this user.
    User,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Local => "local",
            Scope::Project => "project",
            Scope::User => "user",
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SetupClaudeData {
    /// The `claude` CLI that was used.
    pub claude: String,
    /// `mcp` (register the MCP server) or `plugin` (install the pogo plugin).
    pub method: String,
    pub scope: String,
    /// The `claude` command that was run, or would be with --dry-run.
    pub command: Vec<String>,
    /// `added`, `already_registered`, or `planned` (dry run).
    pub status: String,
    pub next_steps: Vec<String>,
}

fn fail(msg: impl Into<String>, hint: &str) -> ErrorBody {
    ErrorBody::new(ErrorCode::CheckFailed, msg).with_hint(hint)
}

/// Whether `p` is a file this user may run (on Unix, an execute bit; shells skip others).
fn runnable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// The first runnable `claude` on PATH, as a shell would pick it.
pub fn find_claude() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["claude.exe", "claude.cmd"]
    } else {
        &["claude"]
    };
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|p| runnable(p))
}

fn run(claude: &Path, args: &[String]) -> Result<std::process::Output, ErrorBody> {
    Command::new(claude).args(args).output().map_err(|e| {
        fail(
            format!("cannot run {}: {e}", claude.display()),
            "Check that Claude Code is installed and runs from a terminal.",
        )
    })
}

/// `pogo_exe` is registered by absolute path, so the server starts even when Claude Code
/// is launched with a PATH that lacks pogo's directory.
pub fn setup_claude(
    pogo_exe: &Path,
    scope: Scope,
    plugin: bool,
    dry_run: bool,
) -> OpResult<SetupClaudeData> {
    let go = || -> Result<SetupClaudeData, ErrorBody> {
        let claude = find_claude().ok_or_else(|| {
            fail(
                "the `claude` command is not on PATH",
                "Install Claude Code (https://code.claude.com), then run `pogo setup claude` again.",
            )
        })?;
        let s = |v: &str| v.to_string();
        let (method, command) = if plugin {
            let cmd = vec![
                s("plugin"),
                s("install"),
                s("pogo"),
                s("--marketplace"),
                s(MARKETPLACE),
                s("--scope"),
                s(scope.as_str()),
            ];
            ("plugin", cmd)
        } else {
            let cmd = vec![
                s("mcp"),
                s("add"),
                s("--scope"),
                s(scope.as_str()),
                s(SERVER),
                s("--"),
                pogo_exe.display().to_string(),
                s("mcp"),
            ];
            ("mcp", cmd)
        };
        // A server already registered under the same name is left alone; replacing it
        // silently could undo someone's own setup.
        let registered = !plugin
            && run(&claude, &[s("mcp"), s("get"), s(SERVER)])?
                .status
                .success();
        let status = if registered {
            "already_registered"
        } else if dry_run {
            "planned"
        } else {
            let out = run(&claude, &command)?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                return Err(fail(
                    format!("`claude {}` failed: {stderr}", command.join(" ")),
                    "Run the command yourself to see the full output.",
                ));
            }
            "added"
        };
        let mut next_steps = vec![s(
            "Restart Claude Code, or run /mcp in a running session, so it connects to pogo.",
        )];
        if registered {
            next_steps.push(s(
                "To re-register (for example after moving pogo), run `claude mcp remove pogo` first.",
            ));
        }
        if plugin {
            next_steps.push(s(
                "The plugin starts `pogo mcp` from PATH; remove any `claude mcp add` registration of pogo so its tools are not listed twice.",
            ));
        } else {
            next_steps.push(format!(
                "Optional: the pogo plugin adds skills that teach Claude the hardware workflow (`pogo setup claude --plugin`, marketplace {MARKETPLACE}); use either it or this registration, not both."
            ));
        }
        next_steps.push(s(
            "Connect a board and run `pogo doctor` to check it is visible.",
        ));
        Ok(SetupClaudeData {
            claude: claude.display().to_string(),
            method: s(method),
            scope: s(scope.as_str()),
            command: std::iter::once(s("claude")).chain(command).collect(),
            status: s(status),
            next_steps,
        })
    };
    go().map_or_else(OpResult::fail, OpResult::ok)
}
