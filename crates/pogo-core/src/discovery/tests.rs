use super::*;
use crate::config::DeviceConfig;

fn usb(vid: u16, pid: u16) -> UsbIds {
    UsbIds {
        vid,
        pid,
        ..UsbIds::default()
    }
}

fn port(name: &str, usb: Option<UsbIds>) -> PortInfo {
    PortInfo {
        port: name.to_string(),
        usb,
    }
}

fn config_with(alias: &str, m: DeviceMatch) -> Config {
    let mut c = Config::default();
    c.devices.insert(
        alias.to_string(),
        DeviceConfig {
            matcher: Some(m),
            ..DeviceConfig::default()
        },
    );
    c
}

#[test]
fn classification_table() {
    let esp = DeviceKind::Esp32Target;
    for (vid, pid) in [
        (0x303a, 0x1001),
        (0x303a, 0x0002),
        (0x10c4, 0xea60),
        (0x1a86, 0x7523),
        (0x1a86, 0x55d4),
        (0x0403, 0x6001),
        (0x0403, 0x6015),
    ] {
        let (kind, hint) = classify(Some(&usb(vid, pid)));
        assert_eq!(kind, esp, "{vid:04x}:{pid:04x}");
        assert!(hint.is_some());
    }
    for (vid, pid) in [(0x10c4, 0xea70), (0x0403, 0x6010), (0x2e8a, 0x000a)] {
        assert_eq!(
            classify(Some(&usb(vid, pid))).0,
            DeviceKind::UnknownSerial,
            "{vid:04x}:{pid:04x}"
        );
    }
    let probe = UsbIds {
        product: Some("pogo-probe".into()),
        ..usb(0x2e8a, 0x000a)
    };
    assert_eq!(classify(Some(&probe)), (DeviceKind::PogoProbe, None));
    assert_eq!(classify(None), (DeviceKind::UnknownSerial, None));
}

#[test]
fn unknown_ports_hidden_unless_all() {
    let ports = vec![
        port("/dev/cu.usbmodem1", Some(usb(0x303a, 0x1001))),
        port("/dev/cu.Bluetooth-Incoming-Port", None),
    ];
    let shown = list_devices(ports.clone(), &Config::default(), false);
    assert_eq!(shown.devices.len(), 1);
    let all = list_devices(ports, &Config::default(), true);
    assert_eq!(all.devices.len(), 2);
}

#[test]
fn json_shape_matches_spec() {
    let ids = UsbIds {
        serial_number: Some("F4:12:FA:9C:3A:10".into()),
        manufacturer: Some("Espressif".into()),
        product: Some("USB JTAG/serial debug unit".into()),
        ..usb(0x303a, 0x1001)
    };
    let config = config_with(
        "target",
        DeviceMatch {
            usb_serial: Some("f4:12:fa:9c:3a:10".into()),
            ..DeviceMatch::default()
        },
    );
    let r = list_devices(vec![port("/dev/ttyACM0", Some(ids))], &config, false);
    assert_eq!(
        serde_json::to_value(&r).unwrap(),
        serde_json::json!({ "devices": [{
            "alias": "target",
            "kind": "esp32_target",
            "port": "/dev/ttyACM0",
            "usb": {
                "vid": "0x303a", "pid": "0x1001",
                "serial_number": "F4:12:FA:9C:3A:10",
                "manufacturer": "Espressif",
                "product": "USB JTAG/serial debug unit"
            },
            "chip_hint": "Espressif native USB (ESP32-S2/S3/C3/C6/H2)",
            "locked_by": null
        }]})
    );
}

#[test]
fn macos_dialin_duplicates_are_dropped_and_output_sorted() {
    let ports = vec![
        port("/dev/tty.usbserial-2", Some(usb(0x10c4, 0xea60))),
        port("/dev/tty.usbmodem1", Some(usb(0x303a, 0x1001))),
        port("/dev/cu.usbmodem1", Some(usb(0x303a, 0x1001))),
    ];
    let r = list_devices(ports, &Config::default(), false);
    let names: Vec<_> = r.devices.iter().map(|d| d.port.as_str()).collect();
    assert_eq!(names, ["/dev/cu.usbmodem1", "/dev/tty.usbserial-2"]);
}

#[test]
fn aliased_unknown_port_is_shown() {
    let config = config_with(
        "bench",
        DeviceMatch {
            port: Some("/dev/ttyS0".into()),
            ..DeviceMatch::default()
        },
    );
    let r = list_devices(vec![port("/dev/ttyS0", None)], &config, false);
    assert_eq!(r.devices[0].alias.as_deref(), Some("bench"));
    assert_eq!(r.devices[0].kind, DeviceKind::UnknownSerial);
}

#[test]
fn every_set_selector_field_must_match() {
    let p = port(
        "/dev/cu.usbmodem1",
        Some(UsbIds {
            serial_number: Some("AA".into()),
            ..usb(0x303a, 0x1001)
        }),
    );
    let m = |usb_serial: Option<&str>, vid: Option<u16>, pid: Option<u16>| DeviceMatch {
        usb_serial: usb_serial.map(str::to_string),
        port: None,
        vid,
        pid,
    };
    assert!(matches(&m(Some("aa"), None, None), &p));
    assert!(matches(&m(None, Some(0x303a), Some(0x1001)), &p));
    assert!(!matches(&m(Some("AA"), Some(0x10c4), None), &p));
    assert!(!matches(&m(None, Some(0x303a), Some(0x0002)), &p));
    assert!(!matches(
        &m(Some("AA"), None, None),
        &port("/dev/ttyS0", None)
    ));
}
