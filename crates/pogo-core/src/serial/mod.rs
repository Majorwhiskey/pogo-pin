//! Serial log handling: line splitting, ANSI stripping, and log parsing.

mod ansi;
pub mod capture;
#[cfg(test)]
mod capture_tests;
pub mod detect;
mod lines;
mod parse;
pub mod result;
pub mod send;
pub mod session;

use schemars::JsonSchema;
use serde::Serialize;

use crate::config::ParserName;

pub use ansi::strip_ansi;
pub use lines::{LineSplitter, RawLine};
pub use parse::{Level, Parsed, parse};

/// One decoded line in a capture's `data.lines`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct LogLine {
    /// Host time the line's first byte arrived, from the start of the capture.
    pub t_ms: u64,
    pub level: Option<Level>,
    pub tag: Option<String>,
    /// Timestamp printed by the device, if the parser found one.
    pub device_ts_ms: Option<u64>,
    pub text: String,
    /// The line was cut at `--max-line-len`. Omitted when false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct DecodeOptions {
    pub parser: ParserName,
    /// Keep ANSI codes in `text` (`--raw`). Parsing always sees the stripped text.
    pub keep_ansi: bool,
    /// Longer lines are cut to this many characters and marked `truncated`.
    pub max_line_len: usize,
}

impl DecodeOptions {
    /// Raw bytes to buffer per line before dropping the rest. Generous because the
    /// character limit applies after UTF-8 decoding and ANSI stripping.
    pub fn max_line_bytes(&self) -> usize {
        self.max_line_len.saturating_mul(8).max(4096)
    }
}

pub fn decode_line(raw: RawLine, opts: &DecodeOptions) -> LogLine {
    let lossy = String::from_utf8_lossy(&raw.bytes);
    let clean = strip_ansi(&lossy);
    let parsed = parse(&clean, opts.parser);
    let mut text = if opts.keep_ansi {
        lossy.into_owned()
    } else {
        clean
    };
    let cut = text.char_indices().nth(opts.max_line_len).map(|(i, _)| i);
    if let Some(i) = cut {
        text.truncate(i);
    }
    LogLine {
        t_ms: raw.t_ms,
        level: parsed.level,
        tag: parsed.tag,
        device_ts_ms: parsed.device_ts_ms,
        text,
        truncated: cut.is_some() || raw.truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(keep_ansi: bool, max_line_len: usize) -> DecodeOptions {
        DecodeOptions {
            parser: ParserName::Auto,
            keep_ansi,
            max_line_len,
        }
    }

    fn raw(bytes: &[u8]) -> RawLine {
        RawLine {
            t_ms: 7,
            bytes: bytes.to_vec(),
            truncated: false,
        }
    }

    #[test]
    fn colored_esp_idf_line_matches_spec_shape() {
        let line = decode_line(
            raw(b"\x1b[0;32mI (412) wifi: connected\x1b[0m"),
            &opts(false, 1000),
        );
        assert_eq!(
            serde_json::to_value(&line).unwrap(),
            serde_json::json!({
                "t_ms": 7, "level": "info", "tag": "wifi", "device_ts_ms": 412,
                "text": "I (412) wifi: connected"
            })
        );
    }

    #[test]
    fn raw_keeps_codes_but_still_parses() {
        let line = decode_line(raw(b"\x1b[0;31mE (1) app: bad\x1b[0m"), &opts(true, 1000));
        assert_eq!(line.text, "\x1b[0;31mE (1) app: bad\x1b[0m");
        assert_eq!(line.level, Some(Level::Error));
    }

    #[test]
    fn garbage_bytes_become_replacement_chars() {
        let line = decode_line(raw(b"\xff\xfeok"), &opts(false, 1000));
        assert_eq!(line.text, "\u{fffd}\u{fffd}ok");
        assert_eq!(line.level, None);
    }

    #[test]
    fn truncation_counts_characters_and_marks_the_cut() {
        let cut = decode_line(raw("héllo".as_bytes()), &opts(false, 3));
        assert_eq!((cut.text.as_str(), cut.truncated), ("hél", true));
        assert_eq!(serde_json::to_value(&cut).unwrap()["truncated"], true);
        let exact = decode_line(raw(b"abc"), &opts(false, 3));
        assert_eq!((exact.text.as_str(), exact.truncated), ("abc", false));
        assert!(
            serde_json::to_value(&exact)
                .unwrap()
                .get("truncated")
                .is_none()
        );
        let capped = RawLine {
            truncated: true,
            ..raw(b"abc")
        };
        assert!(decode_line(capped, &opts(false, 1000)).truncated);
    }
}
