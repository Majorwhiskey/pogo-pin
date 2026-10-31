//! `pogo probe`, `pogo gpio`, and `pogo i2c`: everything that goes through the probe.

use pogo_core::ops::gpio::{self, GpioGetData, GpioPulseData, GpioSetData};
use pogo_core::ops::i2c::{self, I2cReadData, I2cScanData, I2cWriteData};
use pogo_core::ops::probe::{self, ProbeInfoData, SelftestData};

use crate::cli::{GlobalArgs, GpioCommand, I2cCommand, PinState, ProbeCommand, PulseLevel};
use crate::commands::doctor::render_checks;
use crate::commands::target::{reply, selection};
use crate::output::{Render, Reply, Style};

pub fn probe(global: &GlobalArgs, cmd: &ProbeCommand, style: &Style) -> Reply {
    let sel = selection(global);
    match cmd {
        ProbeCommand::Info => reply(probe::info(&sel), style),
        ProbeCommand::Selftest => reply(probe::selftest(&sel), style),
    }
}

pub fn gpio(global: &GlobalArgs, cmd: &GpioCommand, style: &Style) -> Reply {
    let sel = selection(global);
    match cmd {
        GpioCommand::Get { pin } => reply(gpio::get(&sel, pin), style),
        GpioCommand::Set { pin, level } => {
            let state = match level {
                PinState::Low => gpio::PinState::Low,
                PinState::High => gpio::PinState::High,
                PinState::Release => gpio::PinState::Release,
            };
            reply(gpio::set(&sel, pin, state, global.dry_run), style)
        }
        GpioCommand::Pulse {
            pin,
            level,
            duration,
        } => {
            let high = *level == PulseLevel::High;
            reply(
                gpio::pulse(&sel, pin, high, *duration, global.dry_run),
                style,
            )
        }
    }
}

pub fn i2c(global: &GlobalArgs, cmd: &I2cCommand, style: &Style) -> Reply {
    let sel = selection(global);
    match cmd {
        I2cCommand::Scan { bus, range } => reply(i2c::scan(&sel, *bus, range.0, range.1), style),
        I2cCommand::Read {
            addr,
            reg,
            len,
            bus,
        } => reply(i2c::read(&sel, *bus, *addr, *reg, *len), style),
        I2cCommand::Write { addr, bytes, bus } => {
            reply(i2c::write(&sel, *bus, *addr, bytes, global.dry_run), style)
        }
    }
}

fn would(dry_run: bool) -> &'static str {
    if dry_run { "would " } else { "" }
}

impl Render for ProbeInfoData {
    fn render_human(&self, out: &mut String, _style: &Style) {
        let driven = if self.driven_pins.is_empty() {
            "none".to_string()
        } else {
            let pins: Vec<String> = self.driven_pins.iter().map(|p| format!("GP{p}")).collect();
            pins.join(", ")
        };
        out.push_str(&format!(
            "pogo probe {} on {}, protocol {}, up {:.1}s\ndriven pins: {driven}\n",
            self.fw_version,
            self.board,
            self.protocol_version,
            self.uptime_ms as f64 / 1000.0
        ));
    }
}

impl Render for SelftestData {
    fn render_human(&self, out: &mut String, style: &Style) {
        render_checks(&self.checks, out, style);
    }
}

impl Render for GpioGetData {
    fn render_human(&self, out: &mut String, _style: &Style) {
        out.push_str(&format!(
            "{} (GP{}): {}\n",
            self.pin, self.probe_pin, self.level
        ));
    }
}

impl Render for GpioSetData {
    fn render_human(&self, out: &mut String, _style: &Style) {
        let verb = match self.state.as_str() {
            "release" => "release",
            _ => "drive",
        };
        let what = if verb == "release" {
            String::new()
        } else {
            format!(" {}", self.state)
        };
        out.push_str(&format!(
            "{}{verb} {} (GP{}){what}\n",
            would(self.dry_run),
            self.pin,
            self.probe_pin
        ));
    }
}

impl Render for GpioPulseData {
    fn render_human(&self, out: &mut String, _style: &Style) {
        out.push_str(&format!(
            "{}pulse {} (GP{}) {} for {} ms\n",
            would(self.dry_run),
            self.pin,
            self.probe_pin,
            self.level,
            self.duration_ms
        ));
    }
}

impl Render for I2cScanData {
    fn render_human(&self, out: &mut String, style: &Style) {
        out.push_str(&format!(
            "bus {}, {}-{}: {} found\n",
            self.bus,
            self.range[0],
            self.range[1],
            self.found.len()
        ));
        for f in &self.found {
            let hint = if f.may_be.is_empty() {
                String::new()
            } else {
                style.dim(&format!("  may be {}", f.may_be.join(", ")))
            };
            out.push_str(&format!("  {}{hint}\n", f.addr));
        }
    }
}

impl Render for I2cReadData {
    fn render_human(&self, out: &mut String, _style: &Style) {
        let reg = self
            .reg
            .as_ref()
            .map_or(String::new(), |r| format!(" reg {r}"));
        out.push_str(&format!(
            "{}{reg}: {}\n",
            self.addr,
            self.bytes_hex.join(" ")
        ));
    }
}

impl Render for I2cWriteData {
    fn render_human(&self, out: &mut String, _style: &Style) {
        out.push_str(&format!(
            "{}write {} to {} on bus {}\n",
            would(self.dry_run),
            self.bytes_hex.join(" "),
            self.addr,
            self.bus
        ));
    }
}
