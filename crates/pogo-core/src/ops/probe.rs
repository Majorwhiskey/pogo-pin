//! A connected pogo probe for `probe`, `gpio`, and `i2c` commands, and `probe info` and
//! `probe selftest` themselves.

use std::time::Duration;

use pogo_protocol::{Board, ProbeError, Request, Response};
use schemars::JsonSchema;
use serde::Serialize;

use super::OpResult;
use super::target::{Selection, Target, acquire_probe, load_config};
use crate::audit::{self, AuditEntry};
use crate::config::Config;
use crate::doctor::{Check, CheckStatus};
use crate::envelope::{ErrorBody, Warning};
use crate::error::{ErrorCode, PogoError};
use crate::probe::{ProbeClient, REQUEST_TIMEOUT};
use crate::transport::{ReplayTransport, SerialTransport, Transport};

/// The INA226's address when `[power] ina226_addr` is not set.
const DEFAULT_INA226_ADDR: u8 = 0x40;
/// Bus 1 is wired to the INA226 on the probe; bus 0 is the user's.
pub const POWER_BUS: u8 = 1;

/// A locked, handshaken probe. The lock is released when this is dropped.
pub struct Probe {
    pub target: Target,
    client: ProbeClient<Box<dyn Transport>>,
}

impl Probe {
    /// Resolve, lock, open, and say hello. A replay (`POGO_REPLAY`) stands in for the
    /// probe's side of the conversation.
    pub fn open(sel: &Selection, config: Config, command: &str) -> Result<Probe, ErrorBody> {
        let target = acquire_probe(sel, config, command)?;
        #[cfg(test)]
        if let Some(t) = tests::MOCK.with(|m| m.borrow_mut().take()) {
            let client = ProbeClient::connect(t)?;
            return Ok(Probe { target, client });
        }
        let transport: Box<dyn Transport> = match &target.replay {
            Some(path) => Box::new(ReplayTransport::from_file(path)?),
            // USB CDC ignores the baud rate; any value works.
            None => Box::new(SerialTransport::open(&target.device.port, 115_200)?),
        };
        let client = ProbeClient::connect(transport)?;
        Ok(Probe { target, client })
    }

    pub fn fw_version(&self) -> &str {
        &self.client.hello.fw_version
    }

    pub fn protocol_version(&self) -> String {
        let h = &self.client.hello;
        format!("{}.{}", h.protocol_major, h.protocol_minor)
    }

    /// One request; `Response::Err` becomes the matching error.
    pub fn call(&mut self, req: Request) -> Result<Response, ErrorBody> {
        self.call_with(req, REQUEST_TIMEOUT)
    }

    /// Append a mutating action to the audit log (not for replays); a failure to log
    /// becomes a warning.
    pub fn audit<T>(
        &self,
        command: &str,
        args: serde_json::Value,
        result: &Result<T, ErrorBody>,
    ) -> Vec<Warning> {
        if self.target.replay.is_some() {
            return Vec::new();
        }
        let outcome = match result {
            Ok(_) => "ok",
            Err(e) => e.code.as_str(),
        };
        let d = &self.target.device;
        let entry = AuditEntry::new(command, &d.port, d.usb_serial.as_deref(), args, outcome);
        audit::record(&entry)
            .err()
            .map(|e| Warning::new("AUDIT_FAILED", e))
            .into_iter()
            .collect()
    }

    pub fn call_with(&mut self, req: Request, timeout: Duration) -> Result<Response, ErrorBody> {
        match self.client.request(req, timeout)? {
            Response::Err(e) => Err(probe_error(e)),
            r => Ok(r),
        }
    }
}

/// Errors the probe reports, as stable codes. The host checks pins and buses before
/// sending, so a refusal here means the firmware's own allow-list caught it.
pub fn probe_error(e: ProbeError) -> ErrorBody {
    let (code, msg) = match e {
        ProbeError::PinNotAllowed => (ErrorCode::GuardrailRefused, "probe refused the pin"),
        ProbeError::InvalidLevel => (
            ErrorCode::GuardrailRefused,
            "probe refused to drive an open-drain pin high",
        ),
        ProbeError::I2cNack => (ErrorCode::DeviceIo, "no I2C device acknowledged"),
        ProbeError::I2cBus => (ErrorCode::DeviceIo, "I2C bus error (SDA or SCL held low?)"),
        ProbeError::InvalidBus => (ErrorCode::Usage, "probe has no such I2C bus"),
        ProbeError::Busy => (ErrorCode::DeviceBusy, "probe is busy measuring power"),
        ProbeError::PowerUnavailable => (ErrorCode::DeviceIo, "probe has no INA226 or load switch"),
        ProbeError::Unsupported => (
            ErrorCode::ProtocolMismatch,
            "probe firmware does not support this request",
        ),
        ProbeError::PinNotConfigured | ProbeError::BadFrame => {
            (ErrorCode::Protocol, "probe rejected the request")
        }
    };
    ErrorBody::new(code, format!("{msg} ({e:?})"))
}

pub fn unexpected(r: &Response) -> ErrorBody {
    PogoError::Protocol(format!("unexpected response {r:?}")).into()
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ProbeInfoData {
    pub fw_version: String,
    /// `major.minor`.
    pub protocol_version: String,
    /// `rp2040` or `rp2350`.
    pub board: String,
    pub uptime_ms: u64,
    /// Probe GPIO numbers currently driven (not high-Z).
    pub driven_pins: Vec<u8>,
}

fn board_name(b: Board) -> &'static str {
    match b {
        Board::Rp2040 => "rp2040",
        Board::Rp2350 => "rp2350",
    }
}

fn pins_of(mask: u32) -> Vec<u8> {
    (0..32).filter(|p| mask & (1 << p) != 0).collect()
}

pub fn info(sel: &Selection) -> OpResult<ProbeInfoData> {
    let run = || -> Result<ProbeInfoData, ErrorBody> {
        let mut probe = Probe::open(sel, load_config(sel)?, "probe.info")?;
        match probe.call(Request::GetInfo)? {
            Response::Info {
                board,
                uptime_ms,
                driven_pins,
            } => Ok(ProbeInfoData {
                fw_version: probe.fw_version().to_string(),
                protocol_version: probe.protocol_version(),
                board: board_name(board).to_string(),
                uptime_ms,
                driven_pins: pins_of(driven_pins),
            }),
            r => Err(unexpected(&r)),
        }
    };
    run().map_or_else(OpResult::fail, OpResult::ok)
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SelftestData {
    /// Ids: `handshake`, `pins_released`, `ina226`, `i2c_bus0`.
    pub checks: Vec<Check>,
}

pub fn selftest(sel: &Selection) -> OpResult<SelftestData> {
    let config = match load_config(sel) {
        Ok(c) => c,
        Err(e) => return OpResult::fail(e),
    };
    let ina = config
        .power
        .as_ref()
        .and_then(|p| p.ina226_addr)
        .unwrap_or(DEFAULT_INA226_ADDR);
    let mut probe = match Probe::open(sel, config, "probe.selftest") {
        Ok(p) => p,
        Err(e) => return OpResult::fail(e),
    };
    let msg = format!(
        "firmware {}, protocol {}",
        probe.fw_version(),
        probe.protocol_version()
    );
    let mut checks = vec![Check::new("handshake", CheckStatus::Pass, msg)];
    checks.push(pins_check(probe.call(Request::GetInfo)));
    checks.push(ina_check(ina, probe.call(scan(POWER_BUS, ina, ina))));
    checks.push(bus0_check(probe.call(scan(0, 0x08, 0x77))));

    let data = SelftestData { checks };
    let failed: Vec<&str> = data
        .checks
        .iter()
        .filter(|c| c.status == CheckStatus::Fail)
        .map(|c| c.id.as_str())
        .collect();
    let error = (!failed.is_empty()).then(|| {
        ErrorBody::new(
            ErrorCode::CheckFailed,
            format!("probe self-test failed: {}", failed.join(", ")),
        )
    });
    OpResult::ok(data).with_error(error)
}

fn scan(bus: u8, first: u8, last: u8) -> Request {
    Request::I2cScan { bus, first, last }
}

fn failed(id: &str, e: &ErrorBody) -> Check {
    Check::new(id, CheckStatus::Fail, e.message.clone())
}

fn pins_check(r: Result<Response, ErrorBody>) -> Check {
    match r {
        Ok(Response::Info { driven_pins: 0, .. }) => {
            Check::new("pins_released", CheckStatus::Pass, "No pins are driven")
        }
        Ok(Response::Info { driven_pins, .. }) => Check::new(
            "pins_released",
            CheckStatus::Warn,
            format!("Pins still driven: {:?}", pins_of(driven_pins)),
        )
        .hint("Release them with `pogo gpio set <PIN> release`, or replug the probe."),
        Ok(r) => failed("pins_released", &unexpected(&r)),
        Err(e) => failed("pins_released", &e),
    }
}

fn ina_check(addr: u8, r: Result<Response, ErrorBody>) -> Check {
    match r {
        Ok(Response::I2cFound(found)) if found.contains(&addr) => Check::new(
            "ina226",
            CheckStatus::Pass,
            format!("INA226 answers at {addr:#04x}"),
        ),
        Ok(Response::I2cFound(_)) => Check::new(
            "ina226",
            CheckStatus::Warn,
            format!("No INA226 at {addr:#04x}"),
        )
        .hint(
            "Power commands need one; check [power] ina226_addr and the wiring (docs/hardware.md).",
        ),
        Ok(r) => failed("ina226", &unexpected(&r)),
        Err(e) => failed("ina226", &e),
    }
}

fn bus0_check(r: Result<Response, ErrorBody>) -> Check {
    match r {
        Ok(Response::I2cFound(found)) => Check::new(
            "i2c_bus0",
            CheckStatus::Pass,
            format!("Bus 0 idle; {} device(s) answered", found.len()),
        ),
        Ok(r) => failed("i2c_bus0", &unexpected(&r)),
        Err(e) => failed("i2c_bus0", &e)
            .hint("Something may be holding SDA or SCL low; disconnect bus 0 devices and retry."),
    }
}

#[cfg(test)]
#[path = "probe_tests.rs"]
mod tests;
