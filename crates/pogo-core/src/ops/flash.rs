//! `info` and `flash` as ops; flashing is also the first step of `run`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use super::target::{Selection, Target, acquire};
use super::{OpResult, usage};
use crate::audit::{self, AuditEntry};
use crate::deadline::Deadline;
use crate::envelope::{ErrorBody, Warning};
use crate::error::{ErrorCode, PogoError};
use crate::flash::esp::EspFlasher;
use crate::flash::{
    FlashData, FlashImage, FlashOptions, FlashPlan, Flasher, InfoData, check_guardrails,
};

/// `flash`'s deadline when `--timeout` is not given (docs/agent-contract.md §6.6).
pub const DEFAULT_FLASH_TIMEOUT: Duration = Duration::from_secs(120);

/// Everything `flash` can be asked to do (docs/commands.md §7.4).
#[derive(Debug, Clone, Default)]
pub struct FlashRequest {
    pub image: PathBuf,
    pub address: Option<u32>,
    pub chip: Option<String>,
    pub flash_baud: Option<u32>,
    pub bootloader: Option<PathBuf>,
    pub partition_table: Option<PathBuf>,
    pub verify: bool,
    pub erase_all: bool,
    pub confirm: Option<String>,
    pub allow_bootloader_write: bool,
}

/// The image read from disk and checked, ready to flash.
pub struct Prepared {
    pub image: FlashImage,
    pub opts: FlashOptions,
    chip: Option<String>,
    flash_baud: Option<u32>,
    audit_args: serde_json::Value,
}

/// Read the image and turn the request into options; problems here are usage errors.
pub fn prepare(req: &FlashRequest, reset_after: bool) -> Result<Prepared, ErrorBody> {
    let data = std::fs::read(&req.image)
        .map_err(|e| usage(format!("cannot read {}: {e}", req.image.display())))?;
    let image = FlashImage::from_bytes(data, req.address).map_err(usage)?;
    Ok(Prepared {
        image,
        opts: FlashOptions {
            verify: req.verify,
            reset_after,
            erase_all: req.erase_all,
            confirm: req.confirm.clone(),
            allow_bootloader_write: req.allow_bootloader_write,
            bootloader: req.bootloader.clone(),
            partition_table: req.partition_table.clone(),
        },
        chip: req.chip.clone(),
        flash_baud: req.flash_baud,
        audit_args: serde_json::json!({
            "image": req.image.display().to_string(),
            "address": req.address.map(|a| format!("{a:#x}")),
            "erase_all": req.erase_all,
            "allow_bootloader_write": req.allow_bootloader_write,
        }),
    })
}

pub fn info(sel: &Selection) -> OpResult<InfoData> {
    let target = match acquire(sel, "info") {
        Ok(t) => t,
        Err(e) => return OpResult::fail(e),
    };
    if let Err(e) = target.require_hardware() {
        return OpResult::fail(e);
    }
    let d = &target.device;
    let result = EspFlasher::connect(&d.port, d.usb_ids, None, None, false).and_then(|mut f| {
        let info = f.info()?;
        // Leave the target running its application, as it was before.
        f.reset_to_run()?;
        Ok(info)
    });
    match result {
        Ok(info) => OpResult::ok(InfoData {
            device: target.device.clone(),
            info,
        }),
        Err(e) => e.into(),
    }
}

/// `timeout`: `None` means 120 s. `cancel` stops waiting when set (Ctrl-C).
pub fn flash(
    sel: &Selection,
    req: &FlashRequest,
    reset_after: bool,
    dry_run: bool,
    timeout: Option<Duration>,
    cancel: Option<&Arc<AtomicBool>>,
) -> OpResult<FlashData> {
    let prepared = match prepare(req, reset_after) {
        Ok(p) => p,
        Err(e) => return OpResult::fail(e),
    };
    let target = match acquire(sel, "flash") {
        Ok(t) => t,
        Err(e) => return OpResult::fail(e),
    };
    let timeout = timeout.unwrap_or(DEFAULT_FLASH_TIMEOUT);
    match flash_target(&target, &prepared, dry_run, timeout, cancel) {
        Ok((data, warnings)) => OpResult::ok(data).with_warnings(warnings),
        Err(e) => OpResult::fail(e),
    }
}

/// Guardrails, then flash (or plan, with `dry_run`), then audit. A refused request is
/// refused whatever the device, so guardrails come before the hardware check.
pub fn flash_target(
    target: &Target,
    p: &Prepared,
    dry_run: bool,
    timeout: Duration,
    cancel: Option<&Arc<AtomicBool>>,
) -> Result<(FlashData, Vec<Warning>), ErrorBody> {
    check_guardrails(&p.image, &p.opts, &target.device)?;
    if dry_run {
        let data = FlashData {
            device: target.device.clone(),
            report: None,
            plan: Some(FlashPlan::new(&p.image, &p.opts)),
        };
        return Ok((data, Vec::new()));
    }
    target.require_hardware()?;
    let d = &target.device;
    // An explicit chip, else the device's configured one, else auto-detect.
    let chip = p
        .chip
        .clone()
        .or_else(|| target.device_config().and_then(|c| c.chip.clone()));
    let (port, usb_ids, baud) = (d.port.clone(), d.usb_ids, p.flash_baud);
    let (image, opts) = (p.image.clone(), p.opts.clone());
    let result = bounded(Deadline::after(timeout), cancel, move || {
        EspFlasher::connect(&port, usb_ids, chip.as_deref(), baud, opts.verify)
            .and_then(|mut f| f.flash(&image, &opts))
    });
    let outcome = match &result {
        Ok(_) => "ok",
        Err(e) => e.code.as_str(),
    };
    let args = p.audit_args.clone();
    let entry = AuditEntry::new("flash", &d.port, d.usb_serial.as_deref(), args, outcome);
    let warnings = audit::record(&entry)
        .err()
        .map(|e| Warning::new("AUDIT_FAILED", e))
        .into_iter()
        .collect();
    let data = FlashData {
        device: target.device.clone(),
        report: Some(result?),
        plan: None,
    };
    Ok((data, warnings))
}

/// Run `work` on its own thread and stop waiting at the deadline or when `cancel` is set.
/// espflash calls cannot be interrupted and can block indefinitely on a misbehaving
/// port, so a worker that never returns is left behind holding the port; the CLI exits
/// right after, which ends it.
fn bounded<T: Send + 'static>(
    deadline: Deadline,
    cancel: Option<&Arc<AtomicBool>>,
    work: impl FnOnce() -> Result<T, PogoError> + Send + 'static,
) -> Result<T, ErrorBody> {
    const POLL: Duration = Duration::from_millis(50);
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    loop {
        if cancel.is_some_and(|c| c.load(Ordering::SeqCst)) {
            return Err(ErrorBody::new(
                ErrorCode::Interrupted,
                "interrupted while flashing; the board may hold a partial image",
            )
            .with_hint("Flash it again before running it."));
        }
        let Some(left) = deadline.remaining() else {
            return Err(ErrorBody::new(
                ErrorCode::Timeout,
                "flashing did not finish before the deadline; the board or port may be stuck",
            )
            .with_hint(
                "Unplug and replug the board, then retry (raise --timeout for large \
                 images). An MCP server must be restarted: the stuck flash keeps the port.",
            ));
        };
        match rx.recv_timeout(left.min(POLL)) {
            Ok(result) => return result.map_err(ErrorBody::from),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(
                    PogoError::Internal("flash worker stopped without a result".into()).into(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slow() -> Result<u8, PogoError> {
        std::thread::sleep(Duration::from_secs(5));
        Ok(1)
    }

    #[test]
    fn finished_work_returns_its_result() {
        let r = bounded(Deadline::after(Duration::from_secs(5)), None, || Ok(7));
        assert_eq!(r.unwrap(), 7);
        let r: Result<u8, _> = bounded(Deadline::after(Duration::from_secs(5)), None, || {
            Err(PogoError::DeviceIo("gone".into()))
        });
        assert_eq!(r.unwrap_err().code, ErrorCode::DeviceIo);
    }

    #[test]
    fn stuck_work_times_out_at_the_deadline() {
        let start = std::time::Instant::now();
        let r = bounded(Deadline::after(Duration::from_millis(200)), None, slow);
        assert_eq!(r.unwrap_err().code, ErrorCode::Timeout);
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn cancel_stops_waiting() {
        let cancel = Arc::new(AtomicBool::new(false));
        let setter = Arc::clone(&cancel);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            setter.store(true, Ordering::SeqCst);
        });
        let start = std::time::Instant::now();
        let r = bounded(Deadline::after(Duration::from_secs(5)), Some(&cancel), slow);
        assert_eq!(r.unwrap_err().code, ErrorCode::Interrupted);
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
    }
}
