//! `pogo info` and `pogo flash`: clap flags to core requests, and output rendering.

use pogo_core::flash::{FlashData, InfoData};
use pogo_core::ops::flash::{self, FlashRequest};

use crate::cli::{FlashArgs, GlobalArgs};
use crate::commands::target::{reply, selection};
use crate::output::{Render, Reply, Style};

pub fn info(global: &GlobalArgs, style: &Style) -> Reply {
    reply(flash::info(&selection(global)), style)
}

pub fn flash(global: &GlobalArgs, args: &FlashArgs, style: &Style) -> Reply {
    let result = flash::flash(
        &selection(global),
        &flash_request(args),
        !args.no_reset,
        global.dry_run,
    );
    reply(result, style)
}

pub fn flash_request(args: &FlashArgs) -> FlashRequest {
    FlashRequest {
        image: args.image.clone(),
        address: args.address,
        chip: args.chip.clone(),
        flash_baud: args.flash_baud,
        bootloader: args.bootloader.clone(),
        partition_table: args.partition_table.clone(),
        verify: !args.no_verify,
        erase_all: args.erase_all,
        confirm: args.confirm.clone(),
        allow_bootloader_write: args.allow_bootloader_write,
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
