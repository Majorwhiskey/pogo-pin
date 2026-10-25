//! Value parsers for durations, hex values, and I2C address ranges.

use std::time::Duration;

pub(super) fn parse_duration(s: &str) -> Result<Duration, String> {
    humantime::parse_duration(s).map_err(|e| format!("{e} (examples: 500ms, 10s, 2m)"))
}

pub(super) fn parse_hex_u32(s: &str) -> Result<u32, String> {
    let digits = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .ok_or_else(|| format!("expected a hex value like 0x10000, got '{s}'"))?;
    u32::from_str_radix(digits, 16).map_err(|e| format!("invalid hex value '{s}': {e}"))
}

/// Accepts `0x76` or decimal `118`.
pub(super) fn parse_hex_u8(s: &str) -> Result<u8, String> {
    let parsed = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(digits) => u8::from_str_radix(digits, 16),
        None => s.parse(),
    };
    parsed.map_err(|_| format!("expected a byte like 0x76 or 118, got '{s}'"))
}

pub(super) fn parse_i2c_range(s: &str) -> Result<(u8, u8), String> {
    let (first, last) = s
        .split_once('-')
        .ok_or_else(|| format!("expected a range like 0x08-0x77, got '{s}'"))?;
    let (first, last) = (parse_hex_u8(first)?, parse_hex_u8(last)?);
    if first > last || last > 0x7f {
        return Err(format!("invalid 7-bit address range '{s}'"));
    }
    Ok((first, last))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsers() {
        assert_eq!(parse_hex_u32("0x10000"), Ok(0x10000));
        assert!(parse_hex_u32("65536").is_err());
        assert_eq!(parse_hex_u8("0x76"), Ok(0x76));
        assert_eq!(parse_hex_u8("118"), Ok(118));
        assert!(parse_hex_u8("0x100").is_err());
        assert_eq!(parse_i2c_range("0x08-0x77"), Ok((0x08, 0x77)));
        assert!(parse_i2c_range("0x77-0x08").is_err());
        assert!(parse_i2c_range("0x08-0x80").is_err());
        assert_eq!(parse_duration("500ms"), Ok(Duration::from_millis(500)));
    }
}
