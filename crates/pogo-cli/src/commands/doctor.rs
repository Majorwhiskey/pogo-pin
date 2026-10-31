//! `pogo doctor`.

use std::path::PathBuf;

use pogo_core::doctor::{CheckStatus, DoctorData};
use pogo_core::ops::doctor::doctor;

use crate::commands::target::reply;
use crate::output::{Render, Reply, Style};

pub fn run(deep: bool, config: Option<PathBuf>, style: &Style) -> Reply {
    reply(doctor(config, deep), style)
}

impl Render for DoctorData {
    fn render_human(&self, out: &mut String, style: &Style) {
        for c in &self.checks {
            let label = match c.status {
                CheckStatus::Pass => style.green("pass"),
                CheckStatus::Warn => style.yellow("warn"),
                CheckStatus::Fail => style.red("fail"),
            };
            out.push_str(&format!("{label}  {:<13}{}\n", c.id, c.message));
            if let Some(hint) = &c.hint {
                out.push_str(&format!("      {:<13}{}\n", "", style.dim(hint)));
            }
        }
    }
}
