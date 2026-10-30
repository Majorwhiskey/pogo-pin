//! USB enumeration and device classification. Never opens ports.

mod resolve;
#[cfg(test)]
mod tests;

pub use resolve::{ResolvedDevice, resolve};

use schemars::JsonSchema;
use serde::Serialize;

use crate::config::{Config, DeviceMatch};
use crate::error::PogoError;

/// What a connected serial device appears to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Esp32Target,
    PogoProbe,
    UnknownSerial,
}

/// Short description of a device, used in error details such as `AMBIGUOUS_DEVICE`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DeviceSummary {
    pub port: String,
    pub usb_serial: Option<String>,
    pub kind: DeviceKind,
}

/// A serial port as reported by the OS, before classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortInfo {
    pub port: String,
    pub usb: Option<UsbIds>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsbIds {
    pub vid: u16,
    pub pid: u16,
    pub serial_number: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

/// `data` for `pogo devices`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DevicesResult {
    pub devices: Vec<DeviceInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DeviceInfo {
    /// Config alias whose `match` selects this device, if any.
    pub alias: Option<String>,
    pub kind: DeviceKind,
    pub port: String,
    pub usb: Option<UsbDescriptor>,
    pub chip_hint: Option<String>,
    /// Process holding this device's lock, if any.
    pub locked_by: Option<LockHolder>,
}

/// USB descriptor strings; IDs are lowercase hex such as `0x303a`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct UsbDescriptor {
    pub vid: String,
    pub pid: String,
    pub serial_number: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct LockHolder {
    pub pid: u32,
    pub command: String,
}

/// List serial ports from the OS.
pub fn enumerate() -> Result<Vec<PortInfo>, PogoError> {
    let ports = serialport::available_ports()
        .map_err(|e| PogoError::DeviceIo(format!("cannot list serial ports: {e}")))?;
    Ok(ports
        .into_iter()
        .map(|p| PortInfo {
            port: p.port_name,
            usb: match p.port_type {
                serialport::SerialPortType::UsbPort(u) => Some(UsbIds {
                    vid: u.vid,
                    pid: u.pid,
                    serial_number: u.serial_number,
                    manufacturer: u.manufacturer,
                    product: u.product,
                }),
                _ => None,
            },
        })
        .collect())
}

/// Classify by USB IDs (docs/commands.md §7.1). Returns the kind and a chip hint.
pub fn classify(usb: Option<&UsbIds>) -> (DeviceKind, Option<&'static str>) {
    let Some(usb) = usb else {
        return (DeviceKind::UnknownSerial, None);
    };
    let esp = |hint| (DeviceKind::Esp32Target, Some(hint));
    match (usb.vid, usb.pid) {
        (0x303a, _) => esp("Espressif native USB (ESP32-S2/S3/C3/C6/H2)"),
        (0x10c4, 0xea60) => esp("Silicon Labs CP210x bridge (possible ESP32 board)"),
        (0x1a86, 0x7523 | 0x55d4) => esp("WCH CH340/CH9102 bridge (possible ESP32 board)"),
        (0x0403, 0x6001 | 0x6015) => esp("FTDI bridge (possible ESP32 board)"),
        (0x2e8a, _) if usb.product.as_deref() == Some("pogo-probe") => {
            (DeviceKind::PogoProbe, None)
        }
        _ => (DeviceKind::UnknownSerial, None),
    }
}

/// Build the `pogo devices` result. Unknown ports are listed only with `all`, or when a
/// config alias points at them.
pub fn list_devices(ports: Vec<PortInfo>, config: &Config, all: bool) -> DevicesResult {
    let mut devices: Vec<DeviceInfo> = dedupe_macos_dialin(ports)
        .into_iter()
        .map(|p| {
            let (kind, hint) = classify(p.usb.as_ref());
            DeviceInfo {
                alias: alias_for(&p, config),
                kind,
                usb: p.usb.map(|u| UsbDescriptor {
                    vid: format!("{:#06x}", u.vid),
                    pid: format!("{:#06x}", u.pid),
                    serial_number: u.serial_number,
                    manufacturer: u.manufacturer,
                    product: u.product,
                }),
                port: p.port,
                chip_hint: hint.map(str::to_string),
                locked_by: None,
            }
        })
        .filter(|d| all || d.kind != DeviceKind::UnknownSerial || d.alias.is_some())
        .collect();
    devices.sort_by(|a, b| a.port.cmp(&b.port));
    DevicesResult { devices }
}

/// macOS lists each device twice; keep `/dev/cu.*` (opening it never waits for carrier
/// detect) and drop the matching `/dev/tty.*`.
fn dedupe_macos_dialin(ports: Vec<PortInfo>) -> Vec<PortInfo> {
    let callouts: Vec<String> = ports
        .iter()
        .filter_map(|p| p.port.strip_prefix("/dev/cu.").map(str::to_string))
        .collect();
    ports
        .into_iter()
        .filter(|p| {
            p.port
                .strip_prefix("/dev/tty.")
                .is_none_or(|name| !callouts.iter().any(|c| c == name))
        })
        .collect()
}

fn alias_for(port: &PortInfo, config: &Config) -> Option<String> {
    config
        .devices
        .iter()
        .find(|(_, d)| d.matcher.as_ref().is_some_and(|m| matches(m, port)))
        .map(|(name, _)| name.clone())
}

/// Every selector field that is set must match.
pub fn matches(m: &DeviceMatch, port: &PortInfo) -> bool {
    let usb = port.usb.as_ref();
    let serial_ok = m.usb_serial.as_ref().is_none_or(|want| {
        usb.and_then(|u| u.serial_number.as_ref())
            .is_some_and(|have| have.eq_ignore_ascii_case(want))
    });
    let port_ok = m.port.as_ref().is_none_or(|want| *want == port.port);
    let vid_ok = m.vid.is_none_or(|want| usb.is_some_and(|u| u.vid == want));
    let pid_ok = m.pid.is_none_or(|want| usb.is_some_and(|u| u.pid == want));
    serial_ok && port_ok && vid_ok && pid_ok
}
