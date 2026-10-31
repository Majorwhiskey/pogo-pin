//! Property tests: every message round-trips, and frames survive arbitrary chunking.

#![allow(clippy::unwrap_used)]

use heapless::{String, Vec};
use pogo_protocol::codec::{FrameReader, decode, encode};
use pogo_protocol::{
    Board, Frame, Level, MAX_FRAME, PinMode, PowerSample, ProbeError, Request, Response,
};
use proptest::prelude::*;

fn level() -> impl Strategy<Value = Level> {
    prop_oneof![Just(Level::Low), Just(Level::High), Just(Level::Release)]
}

fn bytes<const N: usize>() -> impl Strategy<Value = Vec<u8, N>> {
    proptest::collection::vec(any::<u8>(), 0..=N).prop_map(|v| v.into_iter().collect())
}

fn request() -> impl Strategy<Value = Request> {
    let mode = prop_oneof![
        Just(PinMode::OpenDrain),
        Just(PinMode::PushPull),
        Just(PinMode::Input)
    ];
    prop_oneof![
        (any::<u8>(), any::<u8>()).prop_map(|(a, b)| Request::Hello {
            protocol_major: a,
            protocol_minor: b
        }),
        Just(Request::GetInfo),
        (any::<u8>(), mode).prop_map(|(pin, mode)| Request::GpioConfigure { pin, mode }),
        (any::<u8>(), level()).prop_map(|(pin, level)| Request::GpioSet { pin, level }),
        any::<u8>().prop_map(|pin| Request::GpioGet { pin }),
        (any::<u8>(), level(), any::<u32>()).prop_map(|(pin, level, duration_us)| {
            Request::GpioPulse {
                pin,
                level,
                duration_us,
            }
        }),
        (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(bus, first, last)| Request::I2cScan {
            bus,
            first,
            last
        }),
        (any::<u8>(), any::<u8>(), any::<Option<u8>>(), any::<u8>()).prop_map(
            |(bus, addr, reg, len)| Request::I2cRead {
                bus,
                addr,
                reg,
                len
            }
        ),
        (any::<u8>(), any::<u8>(), bytes::<64>()).prop_map(|(bus, addr, data)| Request::I2cWrite {
            bus,
            addr,
            data
        }),
        (any::<u32>(), any::<u32>()).prop_map(|(interval_us, duration_ms)| {
            Request::PowerStart {
                interval_us,
                duration_ms,
            }
        }),
        Just(Request::PowerStop),
        any::<bool>().prop_map(|on| Request::PowerSwitch { on }),
        Just(Request::ReleaseAll),
    ]
}

fn response() -> impl Strategy<Value = Response> {
    let error = prop_oneof![
        Just(ProbeError::PinNotAllowed),
        Just(ProbeError::I2cNack),
        Just(ProbeError::Busy),
        Just(ProbeError::BadFrame)
    ];
    let sample = (any::<i16>(), any::<u16>())
        .prop_map(|(shunt_raw, bus_raw)| PowerSample { shunt_raw, bus_raw });
    let samples = proptest::collection::vec(sample, 0..=48)
        .prop_map(|v| v.into_iter().collect::<Vec<PowerSample, 48>>());
    prop_oneof![
        (any::<u32>(), any::<u8>(), any::<u8>(), "[ -~]{0,16}").prop_map(|(magic, ma, mi, fw)| {
            Response::Hello {
                magic,
                protocol_major: ma,
                protocol_minor: mi,
                fw_version: String::try_from(fw.as_str()).unwrap(),
            }
        }),
        (any::<bool>(), any::<u64>(), any::<u32>()).prop_map(|(b, uptime_ms, driven_pins)| {
            Response::Info {
                board: if b { Board::Rp2040 } else { Board::Rp2350 },
                uptime_ms,
                driven_pins,
            }
        }),
        Just(Response::Ok),
        level().prop_map(Response::Level),
        bytes::<112>().prop_map(Response::I2cFound),
        bytes::<64>().prop_map(Response::Bytes),
        (any::<u32>(), samples)
            .prop_map(|(index, samples)| Response::PowerChunk { index, samples }),
        (any::<u32>(), any::<u32>())
            .prop_map(|(total, dropped)| Response::PowerDone { total, dropped }),
        error.prop_map(Response::Err),
    ]
}

proptest! {
    #[test]
    fn requests_round_trip(seq in any::<u16>(), body in request()) {
        let frame = Frame { seq, body };
        let mut buf = [0u8; MAX_FRAME];
        let mut wire: std::vec::Vec<u8> = encode(&frame, &mut buf).unwrap().to_vec();
        prop_assert_eq!(decode::<Request>(&mut wire).unwrap(), frame);
    }

    #[test]
    fn responses_round_trip(seq in any::<u16>(), body in response()) {
        let frame = Frame { seq, body };
        let mut buf = [0u8; MAX_FRAME];
        let mut wire: std::vec::Vec<u8> = encode(&frame, &mut buf).unwrap().to_vec();
        prop_assert_eq!(decode::<Response>(&mut wire).unwrap(), frame);
    }

    /// The probe's USB reads arrive in arbitrary pieces; the reader must reassemble them.
    #[test]
    fn frames_survive_arbitrary_chunking(
        bodies in proptest::collection::vec(response(), 1..6),
        cuts in proptest::collection::vec(1usize..40, 1..60),
    ) {
        let frames: std::vec::Vec<Frame<Response>> = bodies
            .into_iter()
            .enumerate()
            .map(|(i, body)| Frame { seq: i as u16, body })
            .collect();
        let mut stream = std::vec::Vec::new();
        for f in &frames {
            let mut buf = [0u8; MAX_FRAME];
            stream.extend_from_slice(encode(f, &mut buf).unwrap());
        }
        let mut reader = FrameReader::new();
        let mut got = std::vec::Vec::new();
        let mut rest = stream.as_slice();
        for cut in cuts.iter().cycle() {
            if rest.is_empty() {
                break;
            }
            let (chunk, tail) = rest.split_at((*cut).min(rest.len()));
            for b in chunk {
                if let Some(f) = reader.push(*b) {
                    got.push(decode::<Response>(f.unwrap()).unwrap());
                }
            }
            rest = tail;
        }
        prop_assert_eq!(got, frames);
    }
}

/// The largest message of each kind must fit: the firmware's buffers are fixed-size.
#[test]
fn worst_case_messages_fit_in_one_frame() {
    let full = |n: usize| -> std::vec::Vec<u8> { std::vec![0xffu8; n] };
    let big_samples: Vec<PowerSample, 48> = (0..48)
        .map(|_| PowerSample {
            shunt_raw: i16::MIN,
            bus_raw: u16::MAX,
        })
        .collect();
    let responses = [
        Response::I2cFound(full(112).into_iter().collect()),
        Response::Bytes(full(64).into_iter().collect()),
        Response::PowerChunk {
            index: u32::MAX,
            samples: big_samples,
        },
        Response::Hello {
            magic: u32::MAX,
            protocol_major: u8::MAX,
            protocol_minor: u8::MAX,
            fw_version: String::try_from("0123456789abcdef").unwrap(),
        },
    ];
    for body in responses {
        let frame = Frame {
            seq: u16::MAX,
            body,
        };
        let mut buf = [0u8; MAX_FRAME];
        let len = encode(&frame, &mut buf).map(|b| b.len());
        assert!(len.is_ok(), "{:?} does not fit", frame.body);
    }
    let write = Frame {
        seq: u16::MAX,
        body: Request::I2cWrite {
            bus: u8::MAX,
            addr: u8::MAX,
            data: full(64).into_iter().collect(),
        },
    };
    let mut buf = [0u8; MAX_FRAME];
    assert!(encode(&write, &mut buf).is_ok());
}
