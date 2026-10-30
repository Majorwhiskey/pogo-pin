//! `pogo run`: flash, reset, and capture from the first boot byte, under one lock.

use pogo_core::ops::DEFAULT_CAPTURE_TIMEOUT;
use pogo_core::ops::run;
use pogo_core::run::RunData;
use pogo_core::serial::LogLine;

use crate::cli::{CaptureArgs, FlashArgs, GlobalArgs};
use crate::commands::flash::flash_request;
use crate::commands::serial::{capture_request, live, stream_line};
use crate::commands::target::{reply, selection};
use crate::output::{Format, Render, Reply, Style};

pub fn run(
    global: &GlobalArgs,
    flash_args: &FlashArgs,
    capture_args: &CaptureArgs,
    no_flash: bool,
    format: Format,
    style: &Style,
) -> Reply {
    let flash = (!no_flash).then(|| flash_request(flash_args));
    let timeout = global.timeout.unwrap_or(DEFAULT_CAPTURE_TIMEOUT);
    let mut stream = |line: &LogLine| stream_line(format, line);
    let result = run::run(
        &selection(global),
        flash.as_ref(),
        &capture_request(capture_args),
        global.dry_run,
        timeout,
        live(&mut stream),
    );
    reply(result, style)
}

impl Render for RunData {
    /// Capture lines were streamed; this summarizes flash and capture.
    fn render_human(&self, out: &mut String, style: &Style) {
        if let Some(plan) = &self.plan {
            out.push_str(&format!(
                "dry run: would flash {} bytes ({}) to {}, then reset and capture\n",
                plan.bytes, plan.image, self.device.port
            ));
            return;
        }
        let mut parts = Vec::new();
        if let Some(f) = &self.flash {
            parts.push(format!("flashed {} in {} ms", f.chip, f.duration_ms));
        }
        if let Some(c) = &self.capture {
            parts.push(match &c.matched {
                Some(m) => format!("matched /{}/ after {} ms", m.pattern, m.t_ms),
                None => format!("captured {} lines", c.line_count),
            });
        }
        out.push_str(&style.dim(&format!("-- {} ({})", parts.join("; "), self.device.port)));
        out.push('\n');
    }
}
