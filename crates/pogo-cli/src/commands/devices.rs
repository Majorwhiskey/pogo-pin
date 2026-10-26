//! `pogo devices`.

use std::path::PathBuf;

use pogo_core::config;
use pogo_core::discovery::{self, DeviceKind, DevicesResult};

use crate::commands::config::search;
use crate::output::{Render, Reply, Style};

pub fn run(all: bool, explicit: Option<PathBuf>, style: &Style) -> Reply {
    let result = search(explicit)
        .and_then(|s| config::load(&s))
        .and_then(|loaded| {
            let ports = discovery::enumerate()?;
            Ok(discovery::list_devices(ports, &loaded.config, all))
        });
    match result {
        Ok(r) => Reply::ok(&r, style),
        Err(e) => Reply::error(&e),
    }
}

impl Render for DevicesResult {
    fn render_human(&self, out: &mut String, style: &Style) {
        if self.devices.is_empty() {
            out.push_str("No devices found.\n");
            return;
        }
        let header = ["PORT", "KIND", "ALIAS", "USB SERIAL", "HINT"].map(str::to_string);
        let rows: Vec<[String; 5]> = self
            .devices
            .iter()
            .map(|d| {
                let kind = match d.kind {
                    DeviceKind::Esp32Target => "esp32_target",
                    DeviceKind::PogoProbe => "pogo_probe",
                    DeviceKind::UnknownSerial => "unknown_serial",
                };
                let serial = d.usb.as_ref().and_then(|u| u.serial_number.clone());
                [
                    d.port.clone(),
                    kind.to_string(),
                    d.alias.clone().unwrap_or_else(|| "-".into()),
                    serial.unwrap_or_else(|| "-".into()),
                    d.chip_hint.clone().unwrap_or_else(|| "-".into()),
                ]
            })
            .collect();
        let mut widths = header.clone().map(|h| h.chars().count());
        for row in &rows {
            for (w, cell) in widths.iter_mut().zip(row) {
                *w = (*w).max(cell.chars().count());
            }
        }
        let line = |cells: &[String; 5]| {
            let mut s = String::new();
            for (i, (cell, w)) in cells.iter().zip(widths).enumerate() {
                if i + 1 == cells.len() {
                    s.push_str(cell);
                } else {
                    s.push_str(&format!("{cell:<w$}  "));
                }
            }
            s
        };
        out.push_str(&style.bold(&line(&header)));
        out.push('\n');
        for row in &rows {
            out.push_str(&line(row));
            out.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pogo_core::config::Config;
    use pogo_core::discovery::{PortInfo, UsbIds, list_devices};

    #[test]
    fn human_table() {
        let ports = vec![
            PortInfo {
                port: "/dev/cu.usbmodem1101".into(),
                usb: Some(UsbIds {
                    vid: 0x303a,
                    pid: 0x1001,
                    serial_number: Some("F4:12:FA:9C:3A:10".into()),
                    ..UsbIds::default()
                }),
            },
            PortInfo {
                port: "/dev/cu.usbserial-0001".into(),
                usb: Some(UsbIds {
                    vid: 0x10c4,
                    pid: 0xea60,
                    ..UsbIds::default()
                }),
            },
        ];
        let r = list_devices(ports, &Config::default(), false);
        let mut out = String::new();
        r.render_human(&mut out, &Style { color: false });
        insta::assert_snapshot!(out);

        let mut empty = String::new();
        DevicesResult { devices: vec![] }.render_human(&mut empty, &Style { color: false });
        assert_eq!(empty, "No devices found.\n");
    }
}
