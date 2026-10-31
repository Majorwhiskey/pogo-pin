//! `pogo power`: measure, and switch the target's supply through the probe.

use pogo_core::ops::power::{self, PowerCycleData, PowerSwitchData};
use pogo_core::ops::power_measure::{self, MeasureRequest, PowerMeasureData};

use crate::cli::{GlobalArgs, PowerCommand};
use crate::commands::target::{reply, selection};
use crate::output::{Render, Reply, Style};

pub fn run(global: &GlobalArgs, cmd: &PowerCommand, style: &Style) -> Reply {
    let sel = selection(global);
    let dry_run = global.dry_run;
    match cmd {
        PowerCommand::Measure {
            duration,
            interval,
            out,
        } => {
            let req = MeasureRequest {
                duration: *duration,
                interval: *interval,
                out: out.clone(),
            };
            reply(power_measure::measure(&sel, &req), style)
        }
        PowerCommand::On => reply(power::set(&sel, true, dry_run), style),
        PowerCommand::Off => reply(power::set(&sel, false, dry_run), style),
        PowerCommand::Cycle { off_time } => reply(power::cycle(&sel, *off_time, dry_run), style),
    }
}

impl Render for PowerMeasureData {
    fn render_human(&self, out: &mut String, style: &Style) {
        out.push_str(&format!(
            "{} samples over {} ms ({} dropped)\n",
            self.samples, self.duration_ms, self.dropped
        ));
        if let (Some(c), Some(v)) = (&self.current_ma, &self.bus_voltage_v) {
            out.push_str(&format!(
                "current  avg {} mA  min {}  max {}  p50 {}  p95 {}\n",
                c.avg, c.min, c.max, c.p50, c.p95
            ));
            out.push_str(&format!(
                "bus      avg {} V  min {}  max {}\n",
                v.avg, v.min, v.max
            ));
            out.push_str(&format!("energy   {} mJ\n", self.energy_mj));
        }
        if let Some(path) = &self.csv_path {
            out.push_str(&style.dim(&format!("samples written to {path}\n")));
        }
    }
}

impl Render for PowerSwitchData {
    fn render_human(&self, out: &mut String, _style: &Style) {
        let verb = if self.dry_run {
            "would switch"
        } else {
            "switched"
        };
        let state = if self.on { "on" } else { "off" };
        out.push_str(&format!("{verb} target power {state}\n"));
    }
}

impl Render for PowerCycleData {
    fn render_human(&self, out: &mut String, _style: &Style) {
        let verb = if self.dry_run {
            "would cycle"
        } else {
            "cycled"
        };
        out.push_str(&format!(
            "{verb} target power (off for {} ms)\n",
            self.off_time_ms
        ));
    }
}
