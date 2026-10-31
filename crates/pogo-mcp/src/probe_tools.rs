//! Tools that go through the pogo probe (docs/mcp.md §9): I2C, GPIO, and power.

use std::time::Duration;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::PogoServer;
use crate::convert::{self, blocking, parse_duration, probe_selection};
use crate::probe_params::{
    GpioParams, GpioPulseParams, GpioSetParams, I2cReadParams, I2cScanParams, I2cWriteParams,
    PinStateParam, PowerAction, PowerMeasureParams, PowerSwitchParams,
};
use pogo_core::ops::power_measure::MeasureRequest;
use pogo_core::ops::{gpio, i2c, power, power_measure};
use pogo_core::{ErrorBody, ErrorCode};

macro_rules! sel {
    ($t:expr) => {
        match probe_selection(&$t) {
            Ok(s) => s,
            Err(e) => return convert::fail(e),
        }
    };
}

#[tool_router(router = probe_router, vis = "pub(crate)")]
impl PogoServer {
    #[tool(
        description = "Scan an I2C bus through the pogo probe and list the 7-bit addresses \
            that acknowledged, with may_be hints of common parts at each address. Bus 0 is \
            the user bus (default), bus 1 the probe's INA226. Read-only.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn i2c_scan(&self, Parameters(p): Parameters<I2cScanParams>) -> CallToolResult {
        let sel = sel!(p.target);
        let (first, last) = (p.first.unwrap_or(0x08), p.last.unwrap_or(0x77));
        blocking(move || i2c::scan(&sel, p.bus, first, last)).await
    }

    #[tool(
        description = "Read 1-64 bytes from an I2C device through the pogo probe, \
            optionally writing a register address first (repeated start). Addresses are \
            decimal numbers. A device that does not acknowledge gives DEVICE_IO.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn i2c_read(&self, Parameters(p): Parameters<I2cReadParams>) -> CallToolResult {
        let sel = sel!(p.target);
        blocking(move || i2c::read(&sel, p.bus, p.addr, p.reg, p.len)).await
    }

    #[tool(
        description = "Write 1-64 bytes to an I2C device on bus 0 through the pogo probe. \
            CHANGES device state. Protected addresses (config i2c_protected_addrs, default \
            0x50-0x57 EEPROMs) and bus 1 are refused with GUARDRAIL_REFUSED.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn i2c_write(&self, Parameters(p): Parameters<I2cWriteParams>) -> CallToolResult {
        let sel = sel!(p.target);
        blocking(move || i2c::write(&sel, p.bus, p.addr, &p.bytes, p.dry_run)).await
    }

    #[tool(
        description = "Read the level (low or high) of a probe pin named in the config's \
            [pins] table. Read-only; undeclared pins are refused.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn gpio_get(&self, Parameters(p): Parameters<GpioParams>) -> CallToolResult {
        let sel = sel!(p.target);
        blocking(move || gpio::get(&sel, &p.pin)).await
    }

    #[tool(
        description = "Drive a probe pin named in the config's [pins] table low or high, \
            or release it (high impedance). CHANGES hardware state, which persists until \
            changed or the probe loses USB. Open-drain pins (such as a target's EN or BOOT) \
            accept only low and release; undeclared pins are refused.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gpio_set(&self, Parameters(p): Parameters<GpioSetParams>) -> CallToolResult {
        let sel = sel!(p.target);
        let state = match p.state {
            PinStateParam::Low => gpio::PinState::Low,
            PinStateParam::High => gpio::PinState::High,
            PinStateParam::Release => gpio::PinState::Release,
        };
        blocking(move || gpio::set(&sel, &p.pin, state, p.dry_run)).await
    }

    #[tool(
        description = "Pulse a probe pin named in the config's [pins] table: hold `level` \
            (default low) for `duration` (default 100ms, max 60s), timed on the probe, then \
            return it to its previous state. Pulsing a target's EN low resets it.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gpio_pulse(&self, Parameters(p): Parameters<GpioPulseParams>) -> CallToolResult {
        let sel = sel!(p.target);
        let high = match p.level {
            None | Some(PinStateParam::Low) => false,
            Some(PinStateParam::High) => true,
            Some(PinStateParam::Release) => {
                return convert::fail(ErrorBody::new(
                    ErrorCode::Usage,
                    "a pulse level is low or high",
                ));
            }
        };
        let duration = match parse_duration(p.duration.as_deref()) {
            Ok(d) => d.unwrap_or(Duration::from_millis(100)),
            Err(e) => return convert::fail(e),
        };
        blocking(move || gpio::pulse(&sel, &p.pin, high, duration, p.dry_run)).await
    }

    #[tool(
        description = "Measure the target's current and bus voltage with the probe's INA226 \
            for `duration` (default 5s) every `interval` (default 1ms). Returns current_ma \
            {avg,min,max,p50,p95}, bus_voltage_v {avg,min,max}, energy_mj, samples, and \
            dropped. The target must be powered through the shunt, not USB. Read-only.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn power_measure(&self, Parameters(p): Parameters<PowerMeasureParams>) -> CallToolResult {
        let sel = sel!(p.target);
        let durations = parse_duration(p.duration.as_deref())
            .and_then(|d| Ok((d, parse_duration(p.interval.as_deref())?)));
        let (duration, interval) = match durations {
            Ok(d) => d,
            Err(e) => return convert::fail(e),
        };
        let req = MeasureRequest {
            duration: duration.unwrap_or(Duration::from_secs(5)),
            interval: interval.unwrap_or(Duration::from_millis(1)),
            out: p.out.map(std::path::PathBuf::from),
        };
        blocking(move || power_measure::measure(&sel, &req)).await
    }

    #[tool(
        description = "Switch the target's supply through the probe's load switch: \
            \"on\", \"off\", or \"cycle\" (off for `off_time`, default 500ms, then on). \
            RESTARTS or STOPS the target. Refused with GUARDRAIL_REFUSED if power would be \
            off for less than min_power_off_time (default 200ms) or more than \
            max_power_cycles_per_minute (default 10) cycles happen.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn power_switch(&self, Parameters(p): Parameters<PowerSwitchParams>) -> CallToolResult {
        let sel = sel!(p.target);
        let dry_run = p.dry_run;
        match p.action {
            PowerAction::On => blocking(move || power::set(&sel, true, dry_run)).await,
            PowerAction::Off => blocking(move || power::set(&sel, false, dry_run)).await,
            PowerAction::Cycle => {
                let off = match parse_duration(p.off_time.as_deref()) {
                    Ok(d) => d.unwrap_or(Duration::from_millis(500)),
                    Err(e) => return convert::fail(e),
                };
                blocking(move || power::cycle(&sel, off, dry_run)).await
            }
        }
    }
}
