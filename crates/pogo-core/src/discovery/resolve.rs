//! Device selection (docs/agent-contract.md §6.7). Never guesses.

use schemars::JsonSchema;
use serde::Serialize;

use super::{DeviceKind, DeviceSummary, PortInfo, classify, list_devices, matches};
use crate::config::Config;
use crate::error::PogoError;

/// The device a command acted on, echoed as `data.device`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ResolvedDevice {
    pub alias: Option<String>,
    pub port: String,
    pub usb_serial: Option<String>,
    #[serde(skip)]
    pub kind: DeviceKind,
    /// USB vendor and product ID, used to pick a reset strategy.
    #[serde(skip)]
    pub usb_ids: Option<(u16, u16)>,
}

/// Resolve `selector` (alias, USB serial, port path, or `auto`) to one device of kind
/// `want`. With no selector, `default_alias` is used when the config defines it,
/// otherwise `auto`.
pub fn resolve(
    selector: Option<&str>,
    default_alias: &str,
    want: DeviceKind,
    ports: Vec<PortInfo>,
    config: &Config,
) -> Result<ResolvedDevice, PogoError> {
    let selector = match selector {
        Some(s) => s,
        None if config.devices.contains_key(default_alias) => default_alias,
        None => "auto",
    };
    let listed = list_devices(ports.clone(), config, true).devices;
    let to_port = |port: &str| ports.iter().find(|p| p.port == port).cloned();

    let candidates: Vec<PortInfo> = if selector == "auto" {
        listed
            .iter()
            .filter(|d| d.kind == want)
            .filter_map(|d| to_port(&d.port))
            .collect()
    } else if let Some(device) = config.devices.get(selector) {
        let Some(m) = &device.matcher else {
            return Err(PogoError::Config(format!(
                "devices.{selector}: missing `match`"
            )));
        };
        listed
            .iter()
            .filter_map(|d| to_port(&d.port))
            .filter(|p| matches(m, p))
            .collect()
    } else {
        let wanted_port = selector.replacen("/dev/tty.", "/dev/cu.", 1);
        let found: Vec<PortInfo> = listed
            .iter()
            .filter_map(|d| to_port(&d.port))
            .filter(|p| {
                p.port == selector
                    || p.port == wanted_port
                    || p.usb
                        .as_ref()
                        .and_then(|u| u.serial_number.as_deref())
                        .is_some_and(|s| s.eq_ignore_ascii_case(selector))
            })
            .collect();
        if found.is_empty() && looks_like_port(selector) {
            // An explicit path the OS doesn't list (a pty, a non-USB UART) is still
            // the user's choice; opening it reports whether it exists.
            vec![PortInfo {
                port: selector.to_string(),
                usb: None,
            }]
        } else {
            found
        }
    };

    match candidates.as_slice() {
        [one] => {
            let alias = list_devices(vec![one.clone()], config, true)
                .devices
                .into_iter()
                .next()
                .and_then(|d| d.alias);
            Ok(ResolvedDevice {
                alias,
                port: one.port.clone(),
                usb_serial: one.usb.as_ref().and_then(|u| u.serial_number.clone()),
                kind: classify(one.usb.as_ref()).0,
                usb_ids: one.usb.as_ref().map(|u| (u.vid, u.pid)),
            })
        }
        [] => Err(PogoError::DeviceNotFound {
            selector: selector.to_string(),
        }),
        many => Err(PogoError::AmbiguousDevice {
            candidates: many
                .iter()
                .map(|p| DeviceSummary {
                    port: p.port.clone(),
                    usb_serial: p.usb.as_ref().and_then(|u| u.serial_number.clone()),
                    kind: classify(p.usb.as_ref()).0,
                })
                .collect(),
        }),
    }
}

fn looks_like_port(s: &str) -> bool {
    s.starts_with('/') || s.to_ascii_uppercase().starts_with("COM")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DeviceConfig, DeviceMatch};
    use crate::discovery::UsbIds;
    use crate::error::ErrorCode;

    fn esp(port: &str, serial: &str) -> PortInfo {
        PortInfo {
            port: port.into(),
            usb: Some(UsbIds {
                vid: 0x303a,
                pid: 0x1001,
                serial_number: Some(serial.into()),
                ..UsbIds::default()
            }),
        }
    }

    fn two_boards() -> Vec<PortInfo> {
        vec![
            esp("/dev/cu.usbmodem1", "AA:AA"),
            esp("/dev/cu.usbmodem2", "BB:BB"),
            PortInfo {
                port: "/dev/cu.Bluetooth".into(),
                usb: None,
            },
        ]
    }

    fn target(serial: &str) -> Config {
        let mut c = Config::default();
        c.devices.insert(
            "target".into(),
            DeviceConfig {
                matcher: Some(DeviceMatch {
                    usb_serial: Some(serial.into()),
                    ..DeviceMatch::default()
                }),
                ..DeviceConfig::default()
            },
        );
        c
    }

    fn go(
        sel: Option<&str>,
        ports: Vec<PortInfo>,
        c: &Config,
    ) -> Result<ResolvedDevice, PogoError> {
        resolve(sel, "target", DeviceKind::Esp32Target, ports, c)
    }

    #[test]
    fn auto_with_one_board_succeeds() {
        let ports = vec![esp("/dev/cu.usbmodem1", "AA:AA")];
        let d = go(None, ports, &Config::default()).unwrap();
        assert_eq!(d.port, "/dev/cu.usbmodem1");
        assert_eq!(d.usb_ids, Some((0x303a, 0x1001)));
    }

    #[test]
    fn auto_with_two_boards_is_ambiguous_and_lists_them() {
        let err = go(Some("auto"), two_boards(), &Config::default()).unwrap_err();
        assert_eq!(err.code(), ErrorCode::AmbiguousDevice);
        let PogoError::AmbiguousDevice { candidates } = err else {
            unreachable!()
        };
        assert_eq!(candidates.len(), 2, "the Bluetooth port is not a candidate");
    }

    #[test]
    fn auto_with_none_is_not_found() {
        let err = go(None, vec![], &Config::default()).unwrap_err();
        assert_eq!(err.code(), ErrorCode::DeviceNotFound);
    }

    #[test]
    fn default_alias_is_used_when_configured() {
        let d = go(None, two_boards(), &target("bb:bb")).unwrap();
        assert_eq!(
            (d.port.as_str(), d.alias.as_deref()),
            ("/dev/cu.usbmodem2", Some("target"))
        );
        let err = go(None, vec![esp("/dev/cu.x", "CC")], &target("bb:bb")).unwrap_err();
        assert_eq!(err.code(), ErrorCode::DeviceNotFound);
    }

    #[test]
    fn serial_number_and_port_selectors() {
        let d = go(Some("aa:aa"), two_boards(), &Config::default()).unwrap();
        assert_eq!(d.port, "/dev/cu.usbmodem1");
        let d = go(Some("/dev/tty.usbmodem2"), two_boards(), &Config::default()).unwrap();
        assert_eq!(d.usb_serial.as_deref(), Some("BB:BB"));
        let d = go(Some("/dev/ttys004"), two_boards(), &Config::default()).unwrap();
        assert_eq!(
            (d.port.as_str(), d.kind),
            ("/dev/ttys004", DeviceKind::UnknownSerial)
        );
        let err = go(Some("ZZ"), two_boards(), &Config::default()).unwrap_err();
        assert_eq!(err.code(), ErrorCode::DeviceNotFound);
    }

    #[test]
    fn data_device_shape() {
        let d = go(Some("aa:aa"), two_boards(), &Config::default()).unwrap();
        assert_eq!(
            serde_json::to_value(&d).unwrap(),
            serde_json::json!({ "alias": null, "port": "/dev/cu.usbmodem1", "usb_serial": "AA:AA" })
        );
    }
}
