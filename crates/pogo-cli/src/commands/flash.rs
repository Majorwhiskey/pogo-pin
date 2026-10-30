//! `pogo info` and `pogo flash`.

use pogo_core::audit::{self, AuditEntry};
use pogo_core::flash::esp::EspFlasher;
use pogo_core::flash::{
    FlashData, FlashImage, FlashOptions, FlashPlan, Flasher, InfoData, check_guardrails,
};
use pogo_core::{ErrorBody, ErrorCode, PogoError, Warning};

use crate::cli::{FlashArgs, GlobalArgs};
use crate::commands::target::{self, Target};
use crate::output::{Render, Reply, Style};

pub fn info(global: &GlobalArgs, style: &Style) -> Reply {
    let target = match target::acquire(global, "info") {
        Ok(t) => t,
        Err(reply) => return *reply,
    };
    if let Err(reply) = target.require_hardware() {
        return *reply;
    }
    let d = &target.device;
    let result = EspFlasher::connect(&d.port, d.usb_ids, None, None, false).and_then(|mut f| {
        let info = f.info()?;
        // Leave the target running its application, as it was before.
        f.reset_to_run()?;
        Ok(info)
    });
    match result {
        Ok(info) => Reply::ok(
            &InfoData {
                device: target.device.clone(),
                info,
            },
            style,
        ),
        Err(e) => Reply::error(&e),
    }
}

/// The parsed image and options for `flash` and `run`.
pub struct FlashRequest {
    pub image: FlashImage,
    pub opts: FlashOptions,
    pub chip: Option<String>,
    pub flash_baud: Option<u32>,
    pub audit_args: serde_json::Value,
}

/// Read the image and turn flags into options; problems here are usage errors.
pub fn request(args: &FlashArgs, reset_after: bool) -> Result<FlashRequest, Box<Reply>> {
    let usage = |msg: String| Box::new(Reply::error(ErrorBody::new(ErrorCode::Usage, msg)));
    let data = std::fs::read(&args.image)
        .map_err(|e| usage(format!("cannot read {}: {e}", args.image.display())))?;
    let image = FlashImage::from_bytes(data, args.address).map_err(usage)?;
    Ok(FlashRequest {
        image,
        opts: FlashOptions {
            verify: !args.no_verify,
            reset_after,
            erase_all: args.erase_all,
            confirm: args.confirm.clone(),
            allow_bootloader_write: args.allow_bootloader_write,
            bootloader: args.bootloader.clone(),
            partition_table: args.partition_table.clone(),
        },
        chip: args.chip.clone(),
        flash_baud: args.flash_baud,
        audit_args: serde_json::json!({
            "image": args.image.display().to_string(),
            "address": args.address.map(|a| format!("{a:#x}")),
            "erase_all": args.erase_all,
            "allow_bootloader_write": args.allow_bootloader_write,
        }),
    })
}

/// Guardrails, then flash (or plan, with `--dry-run`), then audit. Returns the data and
/// any warnings, or the error to report.
pub fn flash_target(
    target: &Target,
    req: &FlashRequest,
    dry_run: bool,
) -> Result<(FlashData, Vec<Warning>), PogoError> {
    check_guardrails(&req.image, &req.opts, &target.device)?;
    if dry_run {
        return Ok((
            FlashData {
                device: target.device.clone(),
                report: None,
                plan: Some(FlashPlan::new(&req.image, &req.opts)),
            },
            Vec::new(),
        ));
    }
    let d = &target.device;
    let result = EspFlasher::connect(
        &d.port,
        d.usb_ids,
        req.chip.as_deref(),
        req.flash_baud,
        req.opts.verify,
    )
    .and_then(|mut f| f.flash(&req.image, &req.opts));
    let outcome = match &result {
        Ok(_) => "ok".to_string(),
        Err(e) => e.code().as_str().to_string(),
    };
    let entry = AuditEntry::new(
        "flash",
        &d.port,
        d.usb_serial.as_deref(),
        req.audit_args.clone(),
        &outcome,
    );
    let warnings = audit::record(&entry)
        .err()
        .map(|e| Warning::new("AUDIT_FAILED", e))
        .into_iter()
        .collect();
    let report = result?;
    Ok((
        FlashData {
            device: target.device.clone(),
            report: Some(report),
            plan: None,
        },
        warnings,
    ))
}

pub fn flash(global: &GlobalArgs, args: &FlashArgs, style: &Style) -> Reply {
    let req = match request(args, !args.no_reset) {
        Ok(r) => r,
        Err(reply) => return *reply,
    };
    let target = match target::acquire(global, "flash") {
        Ok(t) => t,
        Err(reply) => return *reply,
    };
    // A refused request is refused whatever the device, so guardrails come first.
    if let Err(e) = check_guardrails(&req.image, &req.opts, &target.device) {
        return Reply::error(&e);
    }
    if !global.dry_run
        && let Err(reply) = target.require_hardware()
    {
        return *reply;
    }
    match flash_target(&target, &req, global.dry_run) {
        Ok((data, warnings)) => {
            let mut reply = Reply::ok(&data, style);
            reply.warnings = warnings;
            reply
        }
        Err(e) => Reply::error(&e),
    }
}

impl Render for InfoData {
    fn render_human(&self, out: &mut String, _style: &Style) {
        let i = &self.info;
        let mut row = |k: &str, v: String| out.push_str(&format!("{k:<11}{v}\n"));
        row("chip", i.chip.clone());
        row("revision", i.revision.clone().unwrap_or_else(|| "-".into()));
        row("mac", i.mac.clone().unwrap_or_else(|| "-".into()));
        row(
            "flash",
            i.flash_size_bytes
                .map_or("-".into(), |b| format!("{} MB", b / (1024 * 1024))),
        );
        row("crystal", i.crystal.clone().unwrap_or_else(|| "-".into()));
        row("features", i.features.join(", "));
        row("port", self.device.port.clone());
    }
}

impl Render for FlashData {
    fn render_human(&self, out: &mut String, _style: &Style) {
        if let Some(plan) = &self.plan {
            out.push_str(&format!(
                "dry run: would flash {} bytes ({}) to {}{}\n",
                plan.bytes,
                plan.image,
                self.device.port,
                if plan.erase_all {
                    " after erasing all flash"
                } else {
                    ""
                }
            ));
            return;
        }
        if let Some(r) = &self.report {
            for region in &r.regions {
                out.push_str(&format!(
                    "{:<10} {:>8} bytes  {:?}\n",
                    region.address, region.bytes, region.kind
                ));
            }
            out.push_str(&format!(
                "flashed {} in {} ms{}\n",
                r.chip,
                r.duration_ms,
                if r.reset_after {
                    ", reset into run mode"
                } else {
                    ""
                }
            ));
        }
    }
}
