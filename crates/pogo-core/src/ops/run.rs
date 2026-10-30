//! `run` as an op: flash, then reset and capture from the first boot byte, under one lock.

use std::time::Duration;

use super::OpResult;
use super::capture::{CaptureRequest, Live, capture_locked, check_patterns};
use super::flash::{FlashRequest, flash_target, prepare};
use super::outcome::{outcome_error, symbolize_panic};
use super::target::{Selection, acquire};
use crate::flash::FlashImage;
use crate::run::RunData;

/// `flash` is `None` for `--no-flash`.
pub fn run(
    sel: &Selection,
    flash: Option<&FlashRequest>,
    capture: &CaptureRequest,
    dry_run: bool,
    timeout: Duration,
    live: Live,
) -> OpResult<RunData> {
    if let Err(e) = check_patterns(capture) {
        return OpResult::fail(e);
    }
    // Stay in the bootloader after flashing; the capture's own reset then starts the app,
    // so no boot output is lost between flashing and opening the port.
    let prepared = match flash.map(|f| prepare(f, false)).transpose() {
        Ok(p) => p,
        Err(e) => return OpResult::fail(e),
    };
    let target = match acquire(sel, "run") {
        Ok(t) => t,
        Err(e) => return OpResult::fail(e),
    };
    let mut warnings = Vec::new();
    let (report, plan) = match &prepared {
        None => (None, None),
        Some(p) => match flash_target(&target, p, dry_run) {
            Ok((data, w)) => {
                warnings.extend(w);
                (data.report, data.plan)
            }
            Err(e) => return OpResult::fail(e).with_warnings(warnings),
        },
    };
    if dry_run {
        let data = RunData {
            device: target.device.clone(),
            flash: report,
            plan,
            capture: None,
        };
        return OpResult::ok(data).with_warnings(warnings);
    }

    let done = match capture_locked(&target, capture, true, timeout, live) {
        Ok(d) => d,
        Err(e) => return OpResult::fail(e).with_warnings(warnings),
    };
    let mut result = done.out.data.capture;
    warnings.extend(done.out.warnings);
    // The flashed ELF is the natural symbol source when no other was given.
    let flashed_elf = prepared
        .as_ref()
        .is_some_and(|p| matches!(p.image, FlashImage::Elf(_)));
    if flashed_elf
        && let Some(f) = flash
        && result.panic.as_ref().is_some_and(|p| p.frames.is_empty())
    {
        warnings.extend(symbolize_panic(&mut result, Some(&f.image)));
    }
    let error = outcome_error(&result, &capture.until, timeout, done.reconnect);
    let data = RunData {
        device: target.device.clone(),
        flash: report,
        plan: None,
        capture: Some(result),
    };
    OpResult::ok(data).with_warnings(warnings).with_error(error)
}
