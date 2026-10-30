//! `pogo reset`: clap flags to the core op, and output rendering.

use pogo_core::ops::reset::{self, ResetMode as Mode, ResetVia as Via};
use pogo_core::reset::ResetData;

use crate::cli::{GlobalArgs, ResetMode, ResetVia};
use crate::commands::target::{reply, selection};
use crate::output::{Render, Reply, Style};

pub fn reset(global: &GlobalArgs, mode: ResetMode, via: ResetVia, style: &Style) -> Reply {
    let mode = match mode {
        ResetMode::Run => Mode::Run,
        ResetMode::Bootloader => Mode::Bootloader,
    };
    let via = match via {
        ResetVia::Auto => Via::Auto,
        ResetVia::DtrRts => Via::DtrRts,
        ResetVia::Probe => Via::Probe,
    };
    reply(
        reset::reset(&selection(global), mode, via, global.dry_run),
        style,
    )
}

impl Render for ResetData {
    fn render_human(&self, out: &mut String, _style: &Style) {
        let verb = if self.dry_run { "would reset" } else { "reset" };
        out.push_str(&format!(
            "{verb} {} into {} mode via {}{}\n",
            self.device.port,
            self.mode,
            self.via,
            if self.reconnected {
                " (re-enumerated)"
            } else {
                ""
            }
        ));
    }
}
