//! Power ops over a scripted probe, and the `probe_power*.jsonl` replay fixtures.

use std::path::PathBuf;
use std::time::Duration;

use pogo_protocol::heapless::Vec as HVec;
use pogo_protocol::{PROTOCOL_MAJOR, PowerSample, ProbeError, Request, Response};

use super::super::power;
use super::super::probe::tests::{as_jsonl, script, setup, wire};
use super::*;
use crate::error::ErrorCode;

fn s(shunt_raw: i16, bus_raw: u16) -> PowerSample {
    PowerSample { shunt_raw, bus_raw }
}

/// `Hello`, then every response answering request 2, as a stream does.
fn stream_wire(responses: &[Response]) -> Vec<Vec<u8>> {
    let mut frames = wire(PROTOCOL_MAJOR, &[]);
    for body in responses.iter().cloned() {
        let mut buf = [0u8; pogo_protocol::MAX_FRAME];
        let frame = pogo_protocol::Frame { seq: 2, body };
        frames.push(
            pogo_protocol::codec::encode(&frame, &mut buf)
                .unwrap()
                .to_vec(),
        );
    }
    frames
}

fn script_stream(
    responses: &[Response],
) -> std::sync::Arc<std::sync::Mutex<Vec<crate::transport::MockCall>>> {
    let steps = stream_wire(responses)
        .into_iter()
        .map(crate::transport::Step::Rx);
    let t = crate::transport::MockTransport::new(steps);
    let calls = t.calls();
    super::super::probe::tests::MOCK.with(|m| *m.borrow_mut() = Some(Box::new(t)));
    calls
}

/// 42 mA at 3.3 V three times, then a 312 mA peak, with one sample lost in transit
/// and one the probe dropped.
fn session() -> Vec<Response> {
    let chunk = |index, samples: &[PowerSample]| Response::PowerChunk {
        index,
        samples: HVec::from_slice(samples).unwrap(),
    };
    vec![
        chunk(0, &[s(1680, 2640), s(1680, 2640), s(1680, 2640)]),
        chunk(1, &[s(12_480, 2632)]),
        Response::PowerDone {
            total: 5,
            dropped: 1,
        },
    ]
}

#[test]
fn probe_power_fixtures_are_current() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/synthetic");
    let update = std::env::var_os("POGO_UPDATE_FIXTURES").is_some_and(|v| v == "1");
    let fixtures = [
        ("probe_power", stream_wire(&session())),
        (
            "probe_power_busy",
            wire(PROTOCOL_MAJOR, &[Response::Err(ProbeError::Busy)]),
        ),
    ];
    for (name, frames) in fixtures {
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

#[test]
fn ina226_raw_values_convert_with_the_datasheet_lsbs() {
    let (ma, v) = convert(s(1680, 2640), 0.1);
    assert!((ma - 42.0).abs() < 1e-9, "{ma}");
    assert!((v - 3.3).abs() < 1e-9, "{v}");
    let (ma, _) = convert(s(-400, 0), 0.01);
    assert!(
        (ma + 100.0).abs() < 1e-9,
        "negative shunt voltage is reverse current: {ma}"
    );
}

#[test]
fn stats_percentiles_and_energy() {
    let samples: Vec<PowerSample> = (1..=100).map(|i| s(i * 40, 4000)).collect();
    let (c, v, energy) = summarize(&samples, 0.1, Duration::from_millis(10));
    let c = c.unwrap();
    // Sample i draws i mA at 5 V.
    assert_eq!((c.min, c.max, c.p50, c.p95), (1.0, 100.0, 50.0, 95.0));
    assert_eq!(c.avg, 50.5);
    assert_eq!(v.unwrap().avg, 5.0);
    // Σ i mA × 5 V × 10 ms = 5050 × 0.05 mJ.
    assert_eq!(energy, 252.5);
    assert_eq!(
        summarize(&[], 0.1, Duration::from_millis(1)),
        (None, None, 0.0)
    );
}

#[test]
fn measure_sends_start_and_counts_lost_samples() {
    let sel = setup("[power]\nshunt_ohms = 0.1\n");
    let calls = script_stream(&session());
    let req = MeasureRequest {
        duration: Duration::from_millis(5),
        interval: Duration::from_millis(1),
        out: None,
    };
    let r = measure(&sel, &req);
    let d = r.data.unwrap_or_else(|| panic!("{:?}", r.error));
    assert_eq!((d.samples, d.dropped), (4, 2));
    assert_eq!(d.current_ma.unwrap().max, 312.0);
    let calls = calls.lock().unwrap();
    let crate::transport::MockCall::Write(start) = &calls[1] else {
        panic!("{calls:?}")
    };
    let sent: pogo_protocol::Frame<Request> =
        pogo_protocol::codec::decode(&mut start.clone()).unwrap();
    assert_eq!(
        sent.body,
        Request::PowerStart {
            interval_us: 1000,
            duration_ms: 5
        }
    );
}

#[test]
fn measure_rejects_bad_intervals_before_touching_the_probe() {
    let sel = setup("");
    let req = |interval, duration| MeasureRequest {
        interval,
        duration,
        out: None,
    };
    let code = |r: crate::ops::OpResult<PowerMeasureData>| r.error.map(|e| e.code);
    let usage = Some(ErrorCode::Usage);
    assert_eq!(
        code(measure(
            &sel,
            &req(Duration::from_micros(50), Duration::from_secs(1))
        )),
        usage
    );
    assert_eq!(
        code(measure(
            &sel,
            &req(Duration::from_secs(2), Duration::from_secs(1))
        )),
        usage
    );
    assert_eq!(
        code(measure(
            &sel,
            &req(Duration::from_micros(100), Duration::from_secs(3600))
        )),
        usage
    );
}

#[test]
fn power_cycle_switches_off_then_on() {
    let sel = setup("");
    let calls = script(&[Response::Ok, Response::Ok]);
    let r = power::cycle(&sel, Duration::from_millis(200), false);
    assert!(r.error.is_none(), "{:?}", r.error);
    let sent: Vec<Request> = calls
        .lock()
        .unwrap()
        .iter()
        .skip(1)
        .filter_map(|c| match c {
            crate::transport::MockCall::Write(b) => Some(
                pogo_protocol::codec::decode::<Request>(&mut b.clone())
                    .unwrap()
                    .body,
            ),
            _ => None,
        })
        .collect();
    assert_eq!(
        sent,
        [
            Request::PowerSwitch { on: false },
            Request::PowerSwitch { on: true }
        ]
    );
    let r = power::cycle(&sel, Duration::from_millis(50), true);
    assert_eq!(r.error.map(|e| e.code), Some(ErrorCode::GuardrailRefused));
}
