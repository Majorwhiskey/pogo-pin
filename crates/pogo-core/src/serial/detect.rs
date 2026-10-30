//! Recognizers for boot lines, panic markers, and panic details in decoded log text.

use schemars::JsonSchema;
use serde::Serialize;

/// Default fail-on patterns (docs/commands.md §7.6).
pub const DEFAULT_FAIL_ON: &[&str] = &[
    r"Guru Meditation Error",
    r"abort\(\) was called",
    r"=+ PANIC =+",
    r"panicked at",
    r"Brownout detector was triggered",
    r"stack overflow",
    r"Task watchdog got triggered",
];

/// A boot seen in the log, from the ROM's `rst:0x.. (REASON)` line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ResetEvent {
    pub t_ms: u64,
    /// Reset cause register value, e.g. `0x1`.
    pub code: String,
    /// ROM's name for it, e.g. `POWERON`, `TG0WDT_SYS_RST`, `RTCWDT_BROWN_OUT_RESET`.
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PanicLocation {
    pub file: String,
    pub line: u32,
    pub column: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PanicReport {
    /// Index (in the whole capture) of the line that started the panic block.
    pub line_index: usize,
    pub message: Option<String>,
    /// Parsed from a Rust `panicked at file:line:col` message; no ELF needed.
    pub location: Option<PanicLocation>,
    /// Program counters from an ESP-IDF style `Backtrace:` line, e.g. `0x42001234`.
    pub backtrace: Vec<String>,
}

/// `rst:0x1 (POWERON),boot:0xc (SPI_FAST_FLASH_BOOT)` → code `0x1`, reason `POWERON`.
pub fn parse_reset(text: &str, t_ms: u64) -> Option<ResetEvent> {
    let start = text.match_indices("rst:0x").map(|(i, _)| i).find(|&i| {
        text[..i]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_ascii_alphanumeric())
    })?;
    let rest = &text[start + 4..];
    let code_len = 2 + rest[2..].bytes().take_while(u8::is_ascii_hexdigit).count();
    if code_len == 2 {
        return None;
    }
    let code = &rest[..code_len];
    let reason = rest[code_len..]
        .strip_prefix(" (")
        .and_then(|r| r.split_once(')'))
        .map_or("", |(reason, _)| reason);
    Some(ResetEvent {
        t_ms,
        code: code.to_string(),
        reason: reason.to_string(),
    })
}

/// Lines that start a panic block worth collecting and reporting.
pub fn is_panic_marker(text: &str) -> bool {
    text.contains("panicked at")
        || text.contains("Guru Meditation Error")
        || text.contains("abort() was called")
        || (text.contains("PANIC") && text.contains("=="))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockEnd {
    /// The line is part of the panic block.
    Inside,
    /// The line is the block's last (ESP-IDF `Backtrace:`).
    LastLine,
    /// The block ended before this line (a new boot).
    Before,
}

pub fn panic_block_end(text: &str) -> BlockEnd {
    if parse_reset(text, 0).is_some() {
        BlockEnd::Before
    } else if text.trim_start().starts_with("Backtrace:") {
        BlockEnd::LastLine
    } else {
        BlockEnd::Inside
    }
}

/// Build a report from the panic block's lines (`block[0]` is the marker line).
pub fn panic_report(line_index: usize, block: &[&str]) -> PanicReport {
    let mut report = PanicReport {
        line_index,
        message: None,
        location: None,
        backtrace: Vec::new(),
    };
    for (i, text) in block.iter().enumerate() {
        if let Some(after) = text.split_once("panicked at ").map(|(_, a)| a) {
            let (location, inline_msg) = rust_location(after);
            report.location = report.location.or(location);
            let next = block.get(i + 1).map(|s| s.trim()).filter(|s| !s.is_empty());
            report.message = report
                .message
                .or(inline_msg)
                .or_else(|| next.map(str::to_string));
        } else if let Some(pcs) = text.trim_start().strip_prefix("Backtrace:") {
            report.backtrace = pcs
                .split_whitespace()
                .filter_map(|pair| pair.split(':').next())
                .filter(|pc| pc.starts_with("0x"))
                .map(str::to_string)
                .collect();
        } else if report.message.is_none() && text.contains("Guru Meditation Error") {
            report.message = Some(text.trim().to_string());
        }
    }
    report
}

/// After `panicked at `: either `src/main.rs:42:5:` (message on the next line) or the
/// pre-1.73 form `'msg', src/main.rs:42:5`.
fn rust_location(after: &str) -> (Option<PanicLocation>, Option<String>) {
    let (msg, loc) = match after.strip_prefix('\'').and_then(|s| s.rsplit_once("', ")) {
        Some((msg, loc)) => (Some(msg.to_string()), loc),
        None => (None, after),
    };
    let loc = loc.trim().trim_end_matches(':');
    let mut parts = loc.rsplitn(3, ':');
    let (a, b, c) = (parts.next(), parts.next(), parts.next());
    let location = match (a, b, c) {
        (Some(col), Some(line), Some(file)) if !file.is_empty() => {
            match (line.parse(), col.parse()) {
                (Ok(line), Ok(col)) => Some(PanicLocation {
                    file: file.to_string(),
                    line,
                    column: Some(col),
                }),
                _ => None,
            }
        }
        _ => None,
    };
    let location = location.or_else(|| match (a, b) {
        (Some(line), Some(file)) if !file.is_empty() => {
            line.parse().ok().map(|line| PanicLocation {
                file: file.to_string(),
                line,
                column: None,
            })
        }
        _ => None,
    });
    (location, msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_lines() {
        let r = parse_reset("rst:0x1 (POWERON),boot:0xc (SPI_FAST_FLASH_BOOT)", 3).unwrap();
        assert_eq!(
            (r.code.as_str(), r.reason.as_str(), r.t_ms),
            ("0x1", "POWERON", 3)
        );
        let r = parse_reset("rst:0xf (BROWNOUT_RST),boot:0x8 (SPI_FAST_FLASH_BOOT)", 0).unwrap();
        assert_eq!(r.reason, "BROWNOUT_RST");
        let r = parse_reset("ets Jun  8 2016 00:22:57 rst:0x10 (RTCWDT_RTC_RESET)", 0).unwrap();
        assert_eq!(r.code, "0x10");
        assert_eq!(parse_reset("rst:0xZ", 0), None);
        assert_eq!(parse_reset("first:0x1", 0), None);
        assert_eq!(parse_reset("I (5) wifi: ok", 0), None);
    }

    #[test]
    fn rust_panic_new_format() {
        let block = [
            "====================== PANIC ======================",
            "panicked at src/bin/main.rs:47:5:",
            "called `Option::unwrap()` on a `None` value",
        ];
        let r = panic_report(10, &block);
        assert_eq!(
            r.location,
            Some(PanicLocation {
                file: "src/bin/main.rs".into(),
                line: 47,
                column: Some(5)
            })
        );
        assert_eq!(
            r.message.as_deref(),
            Some("called `Option::unwrap()` on a `None` value")
        );
        assert_eq!(r.line_index, 10);
    }

    #[test]
    fn rust_panic_old_format() {
        let r = panic_report(0, &["panicked at 'boom', src/main.rs:9:13"]);
        assert_eq!(r.message.as_deref(), Some("boom"));
        assert_eq!(r.location.unwrap().line, 9);
    }

    #[test]
    fn esp_idf_guru_meditation_backtrace() {
        let block = [
            "Guru Meditation Error: Core  0 panic'ed (LoadProhibited). Exception was unhandled.",
            "PC      : 0x400d1234  PS      : 0x00060030",
            "Backtrace: 0x400d1234:0x3ffb1f50 0x400d5678:0x3ffb1f70 |<-CORRUPTED",
        ];
        let r = panic_report(4, &block);
        assert_eq!(r.backtrace, ["0x400d1234", "0x400d5678"]);
        assert!(r.message.unwrap().starts_with("Guru Meditation Error"));
        assert_eq!(r.location, None);
    }

    #[test]
    fn markers_and_block_ends() {
        assert!(is_panic_marker("panicked at src/main.rs:1:1:"));
        assert!(is_panic_marker("=============== PANIC ==============="));
        assert!(!is_panic_marker("Brownout detector was triggered"));
        assert_eq!(panic_block_end("Backtrace: 0x1:0x2"), BlockEnd::LastLine);
        assert_eq!(
            panic_block_end("rst:0xc (SW_CPU_RESET),boot:0x8"),
            BlockEnd::Before
        );
        assert_eq!(panic_block_end("PC      : 0x400d1234"), BlockEnd::Inside);
    }
}
