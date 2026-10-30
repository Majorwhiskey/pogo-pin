use std::time::{Duration, Instant};

use regex::Regex;

use super::capture::{CaptureOptions, CaptureResult, FaultKind, Outcome, capture};
use super::detect::DEFAULT_FAIL_ON;
use super::{DecodeOptions, Level};
use crate::config::ParserName;
use crate::deadline::Deadline;
use crate::transport::{MockTransport, Step};

fn opts(until: &[&str]) -> CaptureOptions {
    CaptureOptions {
        until: until.iter().map(|p| Regex::new(p).unwrap()).collect(),
        fail_on: DEFAULT_FAIL_ON
            .iter()
            .map(|p| Regex::new(p).unwrap())
            .collect(),
        settle: Duration::ZERO,
        max_lines: 500,
        decode: DecodeOptions {
            parser: ParserName::Auto,
            keep_ansi: false,
            max_line_len: 1000,
        },
        expect_one_boot: false,
        cancel: None,
    }
}

fn run(steps: Vec<Step>, o: &CaptureOptions, timeout_ms: u64) -> CaptureResult {
    let mut t = MockTransport::new(steps);
    let mut seen = 0;
    let r = capture(
        &mut t,
        o,
        Deadline::after(Duration::from_millis(timeout_ms)),
        &mut |_| seen += 1,
    )
    .unwrap();
    assert_eq!(seen, r.line_count, "every line is streamed exactly once");
    r
}

fn texts(r: &CaptureResult) -> Vec<&str> {
    r.lines.iter().map(|l| l.text.as_str()).collect()
}

const BOOT: &str =
    "ESP-ROM:esp32c3-api1-20210207\r\nrst:0x1 (POWERON),boot:0xc (SPI_FAST_FLASH_BOOT)\r\n";

#[test]
fn clean_boot_until_ready() {
    let steps = vec![
        Step::rx(BOOT),
        Step::rx("\x1b[0;32mI (412) wifi: connected\x1b[0m\r\n"),
        Step::rx("READY\r\nnot read\n"),
    ];
    let r = run(steps, &opts(&["READY"]), 2000);
    assert_eq!(r.outcome, Outcome::Matched);
    let m = r.matched.unwrap();
    assert_eq!((m.pattern.as_str(), m.line_index), ("READY", 3));
    assert_eq!(r.line_count, 4);
    assert_eq!(r.boot_count, 1);
    assert_eq!(r.resets[0].reason, "POWERON");
    assert_eq!(r.lines[2].level, Some(Level::Info));
    assert_eq!(r.lines[2].text, "I (412) wifi: connected");
    assert_eq!(r.fault, None);
}

#[test]
fn no_until_runs_to_deadline_and_keeps_the_partial_line() {
    let start = Instant::now();
    let r = run(vec![Step::rx("a\nprompt> ")], &opts(&[]), 60);
    assert!(start.elapsed() >= Duration::from_millis(60));
    assert_eq!(r.outcome, Outcome::Completed);
    assert_eq!(texts(&r), ["a", "prompt> "]);
}

#[test]
fn until_never_matching_is_a_timeout() {
    let r = run(vec![Step::rx("booting\n")], &opts(&["READY"]), 50);
    assert_eq!(r.outcome, Outcome::Timeout);
    assert_eq!(r.matched, None);
}

#[test]
fn silence_is_a_timeout_with_no_lines() {
    let r = run(vec![], &opts(&["READY"]), 30);
    assert_eq!((r.outcome, r.line_count), (Outcome::Timeout, 0));
}

#[test]
fn partial_line_at_deadline_can_match() {
    let r = run(vec![Step::rx("READY")], &opts(&["READY"]), 50);
    assert_eq!(r.outcome, Outcome::Matched);
}

#[test]
fn fail_on_is_checked_before_until() {
    let r = run(
        vec![Step::rx("READY but stack overflow\n")],
        &opts(&["READY"]),
        500,
    );
    assert_eq!(r.outcome, Outcome::Fault);
    assert_eq!(r.matched, None);
    let f = r.fault.unwrap();
    assert_eq!(f.kind, FaultKind::FailOn);
    assert_eq!(f.pattern.as_deref(), Some("stack overflow"));
}

#[test]
fn crash_during_settle_is_a_fault() {
    let mut o = opts(&["READY"]);
    o.settle = Duration::from_millis(300);
    let steps = vec![
        Step::rx("READY\n"),
        Step::delay_ms(20),
        Step::rx("Brownout detector was triggered\n"),
    ];
    let r = run(steps, &o, 2000);
    assert_eq!(r.outcome, Outcome::Fault);
    assert!(r.matched.is_some(), "the match is still reported");
    assert_eq!(r.fault.unwrap().line_index, 1);
}

#[test]
fn clean_settle_waits_then_matches() {
    let mut o = opts(&["READY"]);
    o.settle = Duration::from_millis(80);
    let start = Instant::now();
    let r = run(vec![Step::rx("READY\nidle\n")], &o, 2000);
    let elapsed = start.elapsed();
    assert_eq!(r.outcome, Outcome::Matched);
    assert!(elapsed >= Duration::from_millis(80), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(1000), "{elapsed:?}");
    assert_eq!(r.line_count, 2);
}

#[test]
fn disconnect_is_eof_and_keeps_partial_line() {
    let steps = vec![Step::rx("line\npart"), Step::Disconnect];
    let r = run(steps, &opts(&["READY"]), 2000);
    assert_eq!(r.outcome, Outcome::Eof);
    assert_eq!(texts(&r), ["line", "part"]);
}

#[test]
fn second_boot_after_reset_is_unexpected_reboot() {
    let mut o = opts(&["READY"]);
    o.expect_one_boot = true;
    let steps = vec![
        Step::rx(BOOT),
        Step::rx("rst:0xc (RTC_SW_CPU_RST),boot:0x8 (SPI_FAST_FLASH_BOOT)\n"),
    ];
    let r = run(steps, &o, 2000);
    assert_eq!(r.outcome, Outcome::Fault);
    assert_eq!(r.fault.unwrap().kind, FaultKind::UnexpectedReboot);
    assert_eq!(r.boot_count, 2);
    assert_eq!(r.resets[1].reason, "RTC_SW_CPU_RST");
}

#[test]
fn boots_are_only_counted_without_reset() {
    let steps = vec![Step::rx(BOOT), Step::rx(BOOT), Step::rx("READY\n")];
    let r = run(steps, &opts(&["READY"]), 2000);
    assert_eq!((r.outcome, r.boot_count), (Outcome::Matched, 2));
}

#[test]
fn rust_panic_block_is_collected_until_reboot() {
    let steps = vec![
        Step::rx("READY?\n====================== PANIC ======================\n"),
        Step::rx("panicked at src/bin/main.rs:47:5:\n"),
        Step::rx("called `Option::unwrap()` on a `None` value\n"),
        Step::rx("rst:0xc (RTC_SW_CPU_RST),boot:0x8\nI (1) boot: again\n"),
    ];
    let start = Instant::now();
    let r = run(steps, &opts(&["READY$"]), 5000);
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "block ended at the boot line"
    );
    assert_eq!(r.outcome, Outcome::Fault);
    assert_eq!(r.fault.as_ref().unwrap().kind, FaultKind::Panic);
    let p = r.panic.unwrap();
    assert_eq!(p.line_index, 1);
    let loc = p.location.unwrap();
    assert_eq!(
        (loc.file.as_str(), loc.line, loc.column),
        ("src/bin/main.rs", 47, Some(5))
    );
    assert_eq!(
        p.message.as_deref(),
        Some("called `Option::unwrap()` on a `None` value")
    );
    assert_eq!(
        r.boot_count, 0,
        "the reboot that ends a panic is not counted"
    );
}

#[test]
fn guru_meditation_block_ends_at_backtrace() {
    let steps = vec![
        Step::rx("Guru Meditation Error: Core  0 panic'ed (LoadProhibited).\n"),
        Step::rx("PC      : 0x400d1234\n"),
        Step::rx("Backtrace: 0x400d1234:0x3ffb1f50 0x400d5678:0x3ffb1f70\n"),
        Step::rx("ELF file SHA256: 0000\n"),
    ];
    let r = run(steps, &opts(&[]), 5000);
    assert_eq!(r.outcome, Outcome::Fault);
    assert_eq!(r.panic.unwrap().backtrace, ["0x400d1234", "0x400d5678"]);
    assert_eq!(r.line_count, 3);
}

#[test]
fn panic_collection_is_bounded_by_the_deadline() {
    let start = Instant::now();
    let r = run(
        vec![Step::rx("panicked at src/a.rs:1:1:\n")],
        &opts(&[]),
        100,
    );
    assert!(start.elapsed() < Duration::from_millis(1500));
    assert_eq!(r.outcome, Outcome::Fault);
    assert_eq!(r.panic.unwrap().location.unwrap().line, 1);
}

#[test]
fn excess_lines_are_omitted_from_the_middle() {
    let mut o = opts(&[]);
    o.max_lines = 10;
    let data: String = (0..30).map(|i| format!("l{i}\n")).collect();
    let r = run(vec![Step::rx(data)], &o, 50);
    assert_eq!(
        texts(&r),
        [
            "l0", "l1", "l22", "l23", "l24", "l25", "l26", "l27", "l28", "l29"
        ]
    );
    assert_eq!((r.line_count, r.omitted_lines), (30, 20));
}

#[test]
fn messy_streams_decode_cleanly() {
    let steps = vec![
        Step::rx(b"\xff\x00garbage\r"),
        Step::rx("\nI (5) t: a\rb\r"),
        Step::rx("\nlast"),
    ];
    let r = run(steps, &opts(&[]), 50);
    assert_eq!(
        texts(&r),
        ["\u{fffd}\u{0}garbage", "I (5) t: a", "b", "last"]
    );
}

#[test]
fn cancel_flag_stops_with_partial_data() {
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut o = opts(&["READY"]);
    o.cancel = Some(flag.clone());
    let setter = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(60));
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let start = Instant::now();
    let r = run(vec![Step::rx("booting\n")], &o, 5000);
    setter.join().unwrap();
    assert!(start.elapsed() < Duration::from_millis(1000));
    assert_eq!(r.outcome, Outcome::Interrupted);
    assert_eq!(texts(&r), ["booting"]);
}
