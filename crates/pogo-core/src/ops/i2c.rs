//! `i2c scan|read|write` through the probe.

use pogo_protocol::{Request, Response};
use schemars::JsonSchema;
use serde::Serialize;

use super::probe::{POWER_BUS, Probe, unexpected};
use super::target::{Selection, load_config};
use super::{OpResult, usage};
use crate::config::{Config, parse_addr_range};
use crate::envelope::ErrorBody;
use crate::error::PogoError;

/// Protected when `[guardrails] i2c_protected_addrs` is not set: the 24Cxx EEPROM range.
const DEFAULT_PROTECTED: (u8, u8) = (0x50, 0x57);

/// Common parts at each address, as a hint only; many parts share addresses.
const KNOWN: &[(u8, u8, &[&str])] = &[
    (0x23, 0x23, &["BH1750"]),
    (0x27, 0x27, &["PCF8574 LCD backpack"]),
    (0x29, 0x29, &["VL53L0X"]),
    (0x3c, 0x3d, &["SSD1306"]),
    (0x40, 0x40, &["INA226", "INA219", "Si7021"]),
    (0x44, 0x44, &["SHT31"]),
    (0x48, 0x4b, &["ADS1115", "TMP102"]),
    (0x50, 0x57, &["EEPROM"]),
    (0x5a, 0x5a, &["MLX90614"]),
    (0x68, 0x68, &["MPU6050", "DS3231"]),
    (0x69, 0x69, &["MPU6050"]),
    (0x76, 0x77, &["BME280", "BMP280"]),
];

pub fn hex(b: u8) -> String {
    format!("{b:#04x}")
}

fn check_bus(bus: u8) -> Result<(), ErrorBody> {
    if bus > POWER_BUS {
        return Err(usage(format!(
            "no I2C bus {bus}; bus 0 is the user bus and bus 1 the INA226's"
        )));
    }
    Ok(())
}

fn check_addr(addr: u8) -> Result<(), ErrorBody> {
    if addr > 0x7f {
        return Err(usage(format!("{} is not a 7-bit I2C address", hex(addr))));
    }
    Ok(())
}

fn refused(reason: String) -> ErrorBody {
    PogoError::GuardrailRefused { reason }.into()
}

/// Writes never go to the INA226's bus or to a protected address (docs/guardrails.md §11).
fn check_write(config: &Config, bus: u8, addr: u8) -> Result<(), ErrorBody> {
    if bus == POWER_BUS {
        return Err(refused(
            "bus 1 belongs to the probe's INA226; writes are refused".into(),
        ));
    }
    let ranges: Vec<(u8, u8)> = match &config.guardrails.i2c_protected_addrs {
        Some(list) => list.iter().filter_map(|s| parse_addr_range(s)).collect(),
        None => vec![DEFAULT_PROTECTED],
    };
    if let Some((lo, hi)) = ranges.iter().find(|(lo, hi)| (*lo..=*hi).contains(&addr)) {
        return Err(refused(format!(
            "{} is in the protected range {}-{} ([guardrails] i2c_protected_addrs)",
            hex(addr),
            hex(*lo),
            hex(*hi)
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct I2cFound {
    pub addr: String,
    pub addr_dec: u8,
    /// Parts commonly found at this address; a hint, not an identification.
    pub may_be: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct I2cScanData {
    pub bus: u8,
    /// First and last address scanned, inclusive.
    pub range: [String; 2],
    pub found: Vec<I2cFound>,
}

pub fn scan(sel: &Selection, bus: u8, first: u8, last: u8) -> OpResult<I2cScanData> {
    let run = || -> Result<I2cScanData, ErrorBody> {
        check_bus(bus)?;
        check_addr(last)?;
        if first > last {
            return Err(usage(format!(
                "range {}-{} is empty",
                hex(first),
                hex(last)
            )));
        }
        let mut probe = Probe::open(sel, load_config(sel)?, "i2c.scan")?;
        let found = match probe.call(Request::I2cScan { bus, first, last })? {
            Response::I2cFound(found) => found,
            r => return Err(unexpected(&r)),
        };
        let found = found
            .iter()
            .map(|&a| I2cFound {
                addr: hex(a),
                addr_dec: a,
                may_be: KNOWN
                    .iter()
                    .filter(|(lo, hi, _)| (*lo..=*hi).contains(&a))
                    .flat_map(|(_, _, names)| names.iter().map(|n| n.to_string()))
                    .collect(),
            })
            .collect();
        Ok(I2cScanData {
            bus,
            range: [hex(first), hex(last)],
            found,
        })
    };
    run().map_or_else(OpResult::fail, OpResult::ok)
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct I2cReadData {
    pub bus: u8,
    pub addr: String,
    /// Register written before the read, if any.
    pub reg: Option<String>,
    pub bytes_hex: Vec<String>,
    pub bytes_dec: Vec<u8>,
}

pub fn read(sel: &Selection, bus: u8, addr: u8, reg: Option<u8>, len: u8) -> OpResult<I2cReadData> {
    let run = || -> Result<I2cReadData, ErrorBody> {
        check_bus(bus)?;
        check_addr(addr)?;
        if !(1..=64).contains(&len) {
            return Err(usage("read length must be 1 to 64 bytes"));
        }
        let mut probe = Probe::open(sel, load_config(sel)?, "i2c.read")?;
        let bytes = match probe.call(Request::I2cRead {
            bus,
            addr,
            reg,
            len,
        })? {
            Response::Bytes(b) => b,
            r => return Err(unexpected(&r)),
        };
        Ok(I2cReadData {
            bus,
            addr: hex(addr),
            reg: reg.map(hex),
            bytes_hex: bytes.iter().map(|&b| hex(b)).collect(),
            bytes_dec: bytes.to_vec(),
        })
    };
    run().map_or_else(OpResult::fail, OpResult::ok)
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct I2cWriteData {
    pub bus: u8,
    pub addr: String,
    /// Count of bytes written (or that would be, with dry_run).
    pub bytes_written: usize,
    pub bytes_hex: Vec<String>,
    pub dry_run: bool,
}

pub fn write(
    sel: &Selection,
    bus: u8,
    addr: u8,
    bytes: &[u8],
    dry_run: bool,
) -> OpResult<I2cWriteData> {
    let checked = load_config(sel).and_then(|config| {
        check_bus(bus)?;
        check_addr(addr)?;
        check_write(&config, bus, addr)?;
        let data = pogo_protocol::heapless::Vec::<u8, 64>::from_slice(bytes)
            .map_err(|_| usage("write 1 to 64 bytes"))?;
        if data.is_empty() {
            return Err(usage("write 1 to 64 bytes"));
        }
        Ok((config, data))
    });
    let (config, payload) = match checked {
        Ok(c) => c,
        Err(e) => return OpResult::fail(e),
    };
    let data = I2cWriteData {
        bus,
        addr: hex(addr),
        bytes_written: bytes.len(),
        bytes_hex: bytes.iter().map(|&b| hex(b)).collect(),
        dry_run,
    };
    if dry_run {
        return OpResult::ok(data);
    }
    let mut probe = match Probe::open(sel, config, "i2c.write") {
        Ok(p) => p,
        Err(e) => return OpResult::fail(e),
    };
    let result = probe
        .call(Request::I2cWrite {
            bus,
            addr,
            data: payload,
        })
        .and_then(|r| match r {
            Response::Ok => Ok(()),
            r => Err(unexpected(&r)),
        });
    let args = serde_json::json!({ "bus": bus, "addr": hex(addr), "bytes_hex": data.bytes_hex });
    let warnings = probe.audit("i2c.write", args, &result);
    match result {
        Ok(()) => OpResult::ok(data).with_warnings(warnings),
        Err(e) => OpResult::fail(e).with_warnings(warnings),
    }
}
