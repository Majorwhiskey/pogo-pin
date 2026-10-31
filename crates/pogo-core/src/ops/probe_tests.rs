//! Probe ops over a scripted probe. The `probe_*.jsonl` replay fixtures used by the CLI
//! tests are generated here, so they always match the protocol.

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Duration;

use pogo_protocol::codec::{decode, encode};
use pogo_protocol::heapless::{String as HString, Vec as HVec};
use pogo_protocol::{
    Board, Frame, Level, MAGIC, MAX_FRAME, PROTOCOL_MAJOR, PinMode, ProbeError, Request, Response,
};

use super::super::target::Selection;
use super::super::{gpio, i2c};
use crate::error::ErrorCode;
use crate::transport::{MockCall, MockTransport, Step, Transport};

thread_local! {
    /// When set, the next `Probe::open` on this thread talks to this instead.
    pub static MOCK: RefCell<Option<Box<dyn Transport>>> = const { RefCell::new(None) };
}

fn hello(major: u8) -> Response {
    Response::Hello {
        magic: MAGIC,
        protocol_major: major,
        protocol_minor: 0,
        fw_version: HString::try_from("0.1.0").unwrap(),
    }
}

/// The probe's side of a session: `Hello`, then `responses` with seq 2, 3, ...
pub(crate) fn wire(major: u8, responses: &[Response]) -> Vec<Vec<u8>> {
    std::iter::once(hello(major))
        .chain(responses.iter().cloned())
        .enumerate()
        .map(|(i, body)| {
            let mut buf = [0u8; MAX_FRAME];
            let seq = u16::try_from(i + 1).unwrap();
            encode(&Frame { seq, body }, &mut buf).unwrap().to_vec()
        })
        .collect()
}

fn found(addrs: &[u8]) -> Response {
    Response::I2cFound(HVec::from_slice(addrs).unwrap())
}

fn info(driven_pins: u32) -> Response {
    Response::Info {
        board: Board::Rp2040,
        uptime_ms: 12_345,
        driven_pins,
    }
}

/// Every replay fixture the CLI tests use, by file name.
fn fixtures() -> Vec<(&'static str, Vec<Vec<u8>>)> {
    vec![
        ("probe_info", wire(PROTOCOL_MAJOR, &[info(1 << 2)])),
        (
            "probe_selftest_ok",
            wire(PROTOCOL_MAJOR, &[info(0), found(&[0x40]), found(&[0x76])]),
        ),
        (
            "probe_selftest_bus_stuck",
            wire(
                PROTOCOL_MAJOR,
                &[info(0), found(&[]), Response::Err(ProbeError::I2cBus)],
            ),
        ),
        (
            "probe_ok",
            wire(PROTOCOL_MAJOR, &[Response::Ok, Response::Ok]),
        ),
        (
            "probe_level_low",
            wire(PROTOCOL_MAJOR, &[Response::Level(Level::Low)]),
        ),
        (
            "probe_scan",
            wire(PROTOCOL_MAJOR, &[found(&[0x3c, 0x68, 0x76])]),
        ),
        (
            "probe_bytes",
            wire(
                PROTOCOL_MAJOR,
                &[Response::Bytes(HVec::from_slice(&[0x68]).unwrap())],
            ),
        ),
        (
            "probe_nack",
            wire(PROTOCOL_MAJOR, &[Response::Err(ProbeError::I2cNack)]),
        ),
        ("probe_silent", wire(PROTOCOL_MAJOR, &[])),
        ("probe_old_major", wire(PROTOCOL_MAJOR + 1, &[])),
    ]
}

pub(crate) fn as_jsonl(frames: &[Vec<u8>]) -> String {
    let mut out = String::from(
        "{\"type\":\"header\",\"format\":1,\"pogo_version\":\"0.1.0\",\"port\":\"/dev/cu.usbmodem2101\",\"baud\":115200}\n",
    );
    for (i, f) in frames.iter().enumerate() {
        let hex: String = f.iter().map(|b| format!("{b:02x}")).collect();
        out.push_str(&format!(
            "{{\"type\":\"rx\",\"t_ms\":{i},\"hex\":\"{hex}\"}}\n"
        ));
    }
    out
}

/// `POGO_UPDATE_FIXTURES=1 cargo test -p pogo-core probe_fixtures` rewrites them.
#[test]
fn probe_fixtures_are_current() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/synthetic");
    let update = std::env::var_os("POGO_UPDATE_FIXTURES").is_some_and(|v| v == "1");
    for (name, frames) in fixtures() {
        let path = dir.join(format!("{name}.jsonl"));
        let want = as_jsonl(&frames);
        if update {
            std::fs::write(&path, &want).unwrap();
        }
        let have = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            have, want,
            "{name} is stale; rerun with POGO_UPDATE_FIXTURES=1"
        );
    }
}

/// A config with the docs' example pins, and a selection that skips discovery.
pub(crate) fn setup(extra: &str) -> Selection {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("pogo-probe-ops-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let n = N.fetch_add(1, Ordering::SeqCst);
    let cfg = dir.join(format!("pogo-{n}.toml"));
    let toml = format!(
        "[pins.target_en]\nprobe_pin = 2\nmode = \"open-drain\"\n\
         [pins.led]\nprobe_pin = 10\nmode = \"push-pull\"\n\
         [pins.sense]\nprobe_pin = 11\nmode = \"input\"\n\
         [pins.uart]\nprobe_pin = 0\n{extra}"
    );
    std::fs::write(&cfg, toml).unwrap();
    Selection {
        config: Some(cfg),
        replay: Some(dir.join(format!("unused-{n}.jsonl"))),
        ..Selection::default()
    }
}

/// Script the probe for the next op and return the requests it was sent.
pub(crate) fn script(responses: &[Response]) -> std::sync::Arc<std::sync::Mutex<Vec<MockCall>>> {
    let steps: Vec<Step> = wire(PROTOCOL_MAJOR, responses)
        .into_iter()
        .map(Step::Rx)
        .collect();
    let t = MockTransport::new(steps);
    let calls = t.calls();
    MOCK.with(|m| *m.borrow_mut() = Some(Box::new(t)));
    calls
}

fn sent(calls: &std::sync::Arc<std::sync::Mutex<Vec<MockCall>>>) -> Vec<Request> {
    calls
        .lock()
        .unwrap()
        .iter()
        .filter_map(|c| match c {
            MockCall::Write(bytes) => Some(decode::<Request>(&mut bytes.clone()).unwrap().body),
            _ => None,
        })
        .skip(1)
        .collect()
}

#[test]
fn set_configures_the_declared_mode_then_drives() {
    let sel = setup("");
    let calls = script(&[Response::Ok, Response::Ok]);
    let r = gpio::set(&sel, "target_en", gpio::PinState::Low, false);
    assert!(r.error.is_none(), "{:?}", r.error);
    assert_eq!(
        sent(&calls),
        [
            Request::GpioConfigure {
                pin: 2,
                mode: PinMode::OpenDrain
            },
            Request::GpioSet {
                pin: 2,
                level: Level::Low
            },
        ]
    );
}

#[test]
fn pulse_is_timed_on_the_probe_in_microseconds() {
    let sel = setup("");
    let calls = script(&[Response::Ok, Response::Ok]);
    let r = gpio::pulse(&sel, "led", true, Duration::from_micros(2500), false);
    assert_eq!(r.data.unwrap().duration_ms, 2.5);
    assert_eq!(
        sent(&calls)[1],
        Request::GpioPulse {
            pin: 10,
            level: Level::High,
            duration_us: 2500
        }
    );
}

#[test]
fn pin_guardrails_refuse_before_touching_the_probe() {
    let sel = setup("");
    let code = |r: Option<crate::envelope::ErrorBody>| r.map(|e| e.code);
    let refused = Some(ErrorCode::GuardrailRefused);
    assert_eq!(
        code(gpio::set(&sel, "nope", gpio::PinState::Low, false).error),
        refused
    );
    assert_eq!(
        code(gpio::set(&sel, "target_en", gpio::PinState::High, false).error),
        refused
    );
    assert_eq!(
        code(gpio::set(&sel, "sense", gpio::PinState::Low, false).error),
        refused
    );
    assert_eq!(
        code(gpio::set(&sel, "uart", gpio::PinState::Release, false).error),
        refused
    );
    let r = gpio::pulse(&sel, "target_en", true, Duration::from_millis(1), false);
    assert_eq!(code(r.error), refused);
    let r = gpio::pulse(&sel, "led", true, Duration::from_secs(61), false);
    assert_eq!(code(r.error), Some(ErrorCode::Usage));
    // Nothing was scripted, so reaching the probe would have failed differently.
    assert!(MOCK.with(|m| m.borrow().is_none()));
}

#[test]
fn i2c_write_guardrails() {
    let sel = setup("");
    let code = |r: crate::ops::OpResult<i2c::I2cWriteData>| r.error.map(|e| e.code);
    let refused = Some(ErrorCode::GuardrailRefused);
    assert_eq!(code(i2c::write(&sel, 0, 0x50, &[0], false)), refused);
    assert_eq!(code(i2c::write(&sel, 1, 0x40, &[0], false)), refused);
    assert_eq!(
        code(i2c::write(&sel, 2, 0x10, &[0], false)),
        Some(ErrorCode::Usage)
    );
    assert_eq!(
        code(i2c::write(&sel, 0, 0x80, &[0], false)),
        Some(ErrorCode::Usage)
    );
    let r = i2c::write(&sel, 0, 0x68, &[0x6b, 0x00], true);
    assert_eq!(r.data.unwrap().bytes_hex, ["0x6b", "0x00"]);

    // A configured list replaces the default.
    let sel = setup("[guardrails]\ni2c_protected_addrs = [\"0x68\"]\n");
    assert_eq!(code(i2c::write(&sel, 0, 0x68, &[0], true)), refused);
    assert_eq!(code(i2c::write(&sel, 0, 0x50, &[0], true)), None);
}

#[test]
fn i2c_read_sends_the_register_and_reports_bytes() {
    let sel = setup("");
    let calls = script(&[Response::Bytes(HVec::from_slice(&[0x68, 0x01]).unwrap())]);
    let d = i2c::read(&sel, 0, 0x68, Some(0x75), 2).data.unwrap();
    assert_eq!(
        (d.reg.as_deref(), d.bytes_dec.as_slice()),
        (Some("0x75"), &[0x68, 1][..])
    );
    assert_eq!(
        sent(&calls),
        [Request::I2cRead {
            bus: 0,
            addr: 0x68,
            reg: Some(0x75),
            len: 2
        }]
    );
}

#[test]
fn probe_errors_map_to_stable_codes() {
    use super::probe_error;
    assert_eq!(
        probe_error(ProbeError::PinNotAllowed).code,
        ErrorCode::GuardrailRefused
    );
    assert_eq!(probe_error(ProbeError::I2cNack).code, ErrorCode::DeviceIo);
    assert_eq!(probe_error(ProbeError::Busy).code, ErrorCode::DeviceBusy);
    assert_eq!(probe_error(ProbeError::BadFrame).code, ErrorCode::Protocol);
}
