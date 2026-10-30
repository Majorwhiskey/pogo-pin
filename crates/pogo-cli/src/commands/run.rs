//! `pogo run`: flash, reset, and capture from the first boot byte, under one lock.

use pogo_core::flash::check_guardrails;
use pogo_core::run::RunData;

use crate::cli::{CaptureArgs, FlashArgs, GlobalArgs};
use crate::commands::flash::{flash_target, request};
use crate::commands::serial::{
    DEFAULT_TIMEOUT, capture_locked, check_patterns, outcome_error, symbolize_panic,
};
use crate::commands::target;
use crate::output::{Format, Render, Reply, Style};

pub fn run(
    global: &GlobalArgs,
    flash_args: &FlashArgs,
    capture_args: &CaptureArgs,
    no_flash: bool,
    format: Format,
    style: &Style,
) -> Reply {
    if let Err(reply) = check_patterns(capture_args) {
        return *reply;
    }
    // Stay in the bootloader after flashing; the capture's own reset then starts the app,
    // so no boot output is lost between flashing and opening the port.
    let req = match (!no_flash).then(|| request(flash_args, false)).transpose() {
        Ok(r) => r,
        Err(reply) => return *reply,
    };
    let target = match target::acquire(global, "run") {
        Ok(t) => t,
        Err(reply) => return *reply,
    };
    if let Some(r) = &req
        && let Err(e) = check_guardrails(&r.image, &r.opts, &target.device)
    {
        return Reply::error(&e);
    }
    if req.is_some()
        && !global.dry_run
        && let Err(reply) = target.require_hardware()
    {
        return *reply;
    }
    let mut warnings = Vec::new();
    let (flash, plan) = match &req {
        None => (None, None),
        Some(req) => match flash_target(&target, req, global.dry_run) {
            Ok((data, w)) => {
                warnings.extend(w);
                (data.report, data.plan)
            }
            Err(e) => return Reply::error(&e),
        },
    };
    if global.dry_run {
        let data = RunData {
            device: target.device.clone(),
            flash,
            plan,
            capture: None,
        };
        return Reply::ok(&data, style);
    }

    let timeout = global.timeout.unwrap_or(DEFAULT_TIMEOUT);
    let done = match capture_locked(&target, capture_args, true, format, timeout) {
        Ok(d) => d,
        Err(reply) => return *reply,
    };
    let mut capture = done.out.data.capture;
    warnings.extend(done.out.warnings);
    // The flashed ELF is the natural symbol source when no other was given.
    let flashed_elf =
        matches!(&req, Some(r) if matches!(r.image, pogo_core::flash::FlashImage::Elf(_)));
    if flashed_elf && capture.panic.as_ref().is_some_and(|p| p.frames.is_empty()) {
        warnings.extend(symbolize_panic(&mut capture, Some(&flash_args.image)));
    }
    let error = outcome_error(&capture, &capture_args.until, timeout, done.reconnect);
    let data = RunData {
        device: target.device.clone(),
        flash,
        plan: None,
        capture: Some(capture),
    };
    let mut reply = Reply::ok(&data, style);
    reply.warnings = warnings;
    match error {
        Some(e) => reply.with_error(e),
        None => reply,
    }
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
