//! `info` and `flash` as ops; flashing is also the first step of `run`.

use std::path::PathBuf;

use super::target::{Selection, Target, acquire};
use super::{OpResult, usage};
use crate::audit::{self, AuditEntry};
use crate::envelope::{ErrorBody, Warning};
use crate::flash::esp::EspFlasher;
use crate::flash::{
    FlashData, FlashImage, FlashOptions, FlashPlan, Flasher, InfoData, check_guardrails,
};

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

pub fn flash(
    sel: &Selection,
    req: &FlashRequest,
    reset_after: bool,
    dry_run: bool,
) -> OpResult<FlashData> {
    let prepared = match prepare(req, reset_after) {
        Ok(p) => p,
        Err(e) => return OpResult::fail(e),
    };
    let target = match acquire(sel, "flash") {
        Ok(t) => t,
        Err(e) => return OpResult::fail(e),
    };
    match flash_target(&target, &prepared, dry_run) {
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
    let result = EspFlasher::connect(
        &d.port,
        d.usb_ids,
        p.chip.as_deref(),
        p.flash_baud,
        p.opts.verify,
    )
    .and_then(|mut f| f.flash(&p.image, &p.opts));
    let outcome = match &result {
        Ok(_) => "ok",
        Err(e) => e.code().as_str(),
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
