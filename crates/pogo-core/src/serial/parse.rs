//! Log line parsers (docs/commands.md §7.6). They read fields; the text is kept whole.

use schemars::JsonSchema;
use serde::Serialize;

use crate::config::ParserName;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
    /// ESP-IDF `V` (verbose) and Rust `TRACE`.
    Trace,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    pub level: Option<Level>,
    pub tag: Option<String>,
    pub device_ts_ms: Option<u64>,
}

/// Parse an ANSI-stripped line. `Auto` tries ESP-IDF, then Rust `log`, then gives up.
pub fn parse(text: &str, parser: ParserName) -> Parsed {
    let found = match parser {
        ParserName::Raw => None,
        ParserName::EspIdf => esp_idf(text),
        ParserName::RustLog => rust_log(text),
        ParserName::Auto => esp_idf(text).or_else(|| rust_log(text)),
    };
    found.unwrap_or_default()
}

/// `I (412) wifi: connected` → info, 412 ms, tag `wifi`.
fn esp_idf(text: &str) -> Option<Parsed> {
    let level = match text.as_bytes().first()? {
        b'E' => Level::Error,
        b'W' => Level::Warn,
        b'I' => Level::Info,
        b'D' => Level::Debug,
        b'V' => Level::Trace,
        _ => return None,
    };
    let rest = text.get(1..)?.strip_prefix(" (")?;
    let (ts, rest) = rest.split_once(") ")?;
    if ts.is_empty() || !ts.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (tag, msg) = rest.split_once(':')?;
    if tag.is_empty() || tag.starts_with(' ') || !(msg.is_empty() || msg.starts_with(' ')) {
        return None;
    }
    Some(Parsed {
        level: Some(level),
        tag: Some(tag.to_string()),
        device_ts_ms: ts.parse().ok(),
    })
}

/// `INFO - wifi connected` (esp-println with the `log` feature).
fn rust_log(text: &str) -> Option<Parsed> {
    let (word, rest) = text.split_once(' ')?;
    let level = match word {
        "ERROR" => Level::Error,
        "WARN" => Level::Warn,
        "INFO" => Level::Info,
        "DEBUG" => Level::Debug,
        "TRACE" => Level::Trace,
        _ => return None,
    };
    let rest = rest.trim_start_matches(' ').strip_prefix('-')?;
    if !(rest.is_empty() || rest.starts_with(' ')) {
        return None;
    }
    Some(Parsed {
        level: Some(level),
        ..Parsed::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str, parser: ParserName) -> Parsed {
        parse(text, parser)
    }

    #[test]
    fn esp_idf_lines() {
        let got = p("I (412) wifi: connected", ParserName::EspIdf);
        assert_eq!(
            got,
            Parsed {
                level: Some(Level::Info),
                tag: Some("wifi".into()),
                device_ts_ms: Some(412)
            }
        );
        let e = p("E (12) app_main: x: y", ParserName::EspIdf);
        assert_eq!(e.level, Some(Level::Error));
        assert_eq!(e.tag.as_deref(), Some("app_main"));
        assert_eq!(p("V (0) t:", ParserName::EspIdf).level, Some(Level::Trace));
    }

    #[test]
    fn esp_idf_rejects_lookalikes() {
        for text in [
            "ESP-ROM:esp32c3-api1-20210207",
            "I (abc) wifi: x",
            "I () wifi: x",
            "I (12) : x",
            "I (12) wifi:x",
            "Info (12) wifi: x",
            "rst:0x1 (POWERON),boot:0xc (SPI_FAST_FLASH_BOOT)",
        ] {
            assert_eq!(p(text, ParserName::EspIdf), Parsed::default(), "{text}");
        }
    }

    #[test]
    fn rust_log_lines() {
        assert_eq!(
            p("INFO - wifi connected", ParserName::RustLog).level,
            Some(Level::Info)
        );
        assert_eq!(
            p("WARN  - low", ParserName::RustLog).level,
            Some(Level::Warn)
        );
        assert_eq!(p("TRACE -", ParserName::RustLog).level, Some(Level::Trace));
        assert_eq!(p("INFO-x", ParserName::RustLog), Parsed::default());
        assert_eq!(p("INFO -x", ParserName::RustLog), Parsed::default());
        assert_eq!(p("NOTICE - x", ParserName::RustLog), Parsed::default());
    }

    #[test]
    fn auto_tries_both_and_raw_parses_nothing() {
        assert_eq!(p("W (5) t: m", ParserName::Auto).level, Some(Level::Warn));
        assert_eq!(p("ERROR - m", ParserName::Auto).level, Some(Level::Error));
        assert_eq!(p("READY", ParserName::Auto), Parsed::default());
        assert_eq!(p("I (5) t: m", ParserName::Raw), Parsed::default());
        assert_eq!(p("I (5) t: m", ParserName::RustLog), Parsed::default());
    }
}
