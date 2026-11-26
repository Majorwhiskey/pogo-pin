//! `run` as an op: flash, then reset and capture from the first boot byte, under one lock.

use std::path::Path;
use std::time::Duration;

use super::capture::{CaptureRequest, Live, capture_locked, check_patterns};
use super::flash::{DEFAULT_FLASH_TIMEOUT, FlashRequest, flash_target, prepare};
use super::outcome::outcome_error;
use super::target::{Selection, acquire};
use super::{DEFAULT_CAPTURE_TIMEOUT, OpResult};
use crate::flash::FlashImage;
use crate::run::RunData;

/// `flash` is `None` for `--no-flash`. `timeout` (the capture's): `None` means the
/// config's `defaults.timeout`, else 10 s.
pub fn run(
    sel: &Selection,
    flash: Option<&FlashRequest>,
    capture: &CaptureRequest,
    dry_run: bool,
    timeout: Option<Duration>,
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
        Some(p) => match flash_target(
            &target,
            p,
            dry_run,
            DEFAULT_FLASH_TIMEOUT,
            live.cancel.as_ref(),
        ) {
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

    let flashed_elf = match (&prepared, flash) {
        (Some(p), Some(f)) if matches!(p.image, FlashImage::Elf(_)) => Some(f.image.as_path()),
        _ => None,
    };
    let capture = &with_symbol_elf(capture, flashed_elf);
    let timeout = target.timeout(timeout, DEFAULT_CAPTURE_TIMEOUT);
    let done = match capture_locked(sel, &target, capture, true, timeout, live) {
        Ok(d) => d,
        Err(e) => return OpResult::fail(e).with_warnings(warnings),
    };
    let result = done.out.data.capture;
    warnings.extend(done.out.warnings);
    let error = outcome_error(&result, &capture.until, timeout, done.reconnect);
    let data = RunData {
        device: target.device.clone(),
        flash: report,
        plan: None,
        capture: Some(result),
    };
    OpResult::ok(data).with_warnings(warnings).with_error(error)
}

/// Symbolize with an explicit `--elf`, else the ELF just flashed. Only then the
/// device's configured `elf`, which may be an older build than the one running.
fn with_symbol_elf(capture: &CaptureRequest, flashed: Option<&Path>) -> CaptureRequest {
    CaptureRequest {
        elf: capture.elf.clone().or(flashed.map(Path::to_path_buf)),
        ..capture.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn flashed_elf_beats_the_configured_one_but_not_an_explicit_one() {
        let flashed = Path::new("new.elf");
        let none = CaptureRequest::default();
        assert_eq!(
            with_symbol_elf(&none, Some(flashed)).elf,
            Some(PathBuf::from("new.elf"))
        );
        assert_eq!(with_symbol_elf(&none, None).elf, None);
        let explicit = CaptureRequest {
            elf: Some("mine.elf".into()),
            ..CaptureRequest::default()
        };
        assert_eq!(
            with_symbol_elf(&explicit, Some(flashed)).elf,
            Some(PathBuf::from("mine.elf"))
        );
    }
}
