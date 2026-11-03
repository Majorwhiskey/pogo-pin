//! `pogo setup claude`.

use pogo_core::ops::setup::{self, Scope, SetupClaudeData};
use pogo_core::{ErrorBody, ErrorCode};

use crate::cli::{GlobalArgs, SetupCommand, SetupScope};
use crate::commands::target::reply;
use crate::output::{Render, Reply, Style};

pub fn run(global: &GlobalArgs, cmd: &SetupCommand, style: &Style) -> Reply {
    let SetupCommand::Claude { scope, plugin } = cmd;
    let scope = match scope {
        SetupScope::Local => Scope::Local,
        SetupScope::Project => Scope::Project,
        SetupScope::User => Scope::User,
    };
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            return Reply::error(ErrorBody::new(
                ErrorCode::Internal,
                format!("cannot find this pogo binary: {e}"),
            ));
        }
    };
    reply(
        setup::setup_claude(&exe, scope, *plugin, global.dry_run),
        style,
    )
}

impl Render for SetupClaudeData {
    fn render_human(&self, out: &mut String, style: &Style) {
        match self.status.as_str() {
            "already_registered" => {
                out.push_str("pogo is already registered with Claude Code; nothing changed\n");
            }
            status => {
                let what = if status == "added" {
                    style.green("done")
                } else {
                    "would run".to_string()
                };
                out.push_str(&format!("{what}: {}\n", self.command.join(" ")));
            }
        }
        for step in &self.next_steps {
            out.push_str(&format!("  - {step}\n"));
        }
    }
}
