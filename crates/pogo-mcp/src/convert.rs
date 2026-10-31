//! Tool arguments to core requests, and core results to MCP tool results.

use std::path::PathBuf;
use std::time::Duration;

use pogo_core::config::ParserName;
use pogo_core::ops::capture::CaptureRequest;
use pogo_core::ops::flash::FlashRequest;
use pogo_core::ops::send::SendRequest;
use pogo_core::ops::{OpResult, Selection};
use pogo_core::serial::send::parse_hex;
use pogo_core::{ErrorBody, ErrorCode};
use rmcp::model::CallToolResult;
use serde::Serialize;

use crate::params::{CaptureParams, FlashParams, LineEndingParam, ParserParam, SerialSendParams};

/// MCP has no flags, so the config comes from `POGO_CONFIG` or the usual search.
pub fn selection(device: Option<String>) -> Selection {
    Selection {
        config: std::env::var_os("POGO_CONFIG").map(PathBuf::from),
        device,
        wait_lock: None,
        replay: std::env::var_os("POGO_REPLAY").map(PathBuf::from),
    }
}

fn usage(msg: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorCode::Usage, msg)
}

pub fn parse_duration(s: Option<&str>, default: Duration) -> Result<Duration, ErrorBody> {
    match s {
        None => Ok(default),
        Some(s) => humantime::parse_duration(s).map_err(|e| {
            usage(format!(
                "invalid duration '{s}': {e}; use forms like 500ms, 10s"
            ))
        }),
    }
}

fn parse_address(s: &str) -> Result<u32, ErrorBody> {
    let digits = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    u32::from_str_radix(digits, 16)
        .map_err(|_| usage(format!("invalid address '{s}'; use hex like 0x10000")))
}

pub fn flash_request(p: &FlashParams) -> Result<FlashRequest, ErrorBody> {
    Ok(FlashRequest {
        image: PathBuf::from(&p.image),
        address: p.address.as_deref().map(parse_address).transpose()?,
        chip: p.chip.clone(),
        flash_baud: p.flash_baud,
        bootloader: None,
        partition_table: None,
        verify: !p.no_verify,
        // A full chip erase is never exposed over MCP (docs/mcp.md §9).
        erase_all: false,
        confirm: None,
        allow_bootloader_write: p.allow_bootloader_write,
    })
}

pub fn capture_request(c: &CaptureParams) -> Result<CaptureRequest, ErrorBody> {
    let defaults = CaptureRequest::default();
    Ok(CaptureRequest {
        baud: c.baud,
        until: c.until.clone(),
        fail_on: c.fail_on.clone(),
        no_default_fail_patterns: c.no_default_fail_patterns,
        settle: c
            .settle
            .as_deref()
            .map(|s| parse_duration(Some(s), Duration::ZERO))
            .transpose()?,
        max_lines: c.max_lines.unwrap_or(defaults.max_lines),
        parser: match c.parser {
            ParserParam::Auto => ParserName::Auto,
            ParserParam::EspIdf => ParserName::EspIdf,
            ParserParam::RustLog => ParserName::RustLog,
            ParserParam::Raw => ParserName::Raw,
        },
        elf: c.elf.as_ref().map(PathBuf::from),
        ..defaults
    })
}

pub fn send_request(p: &SerialSendParams) -> Result<SendRequest, ErrorBody> {
    let mut bytes = if p.hex {
        parse_hex(&p.data).map_err(usage)?
    } else {
        p.data.as_bytes().to_vec()
    };
    bytes.extend_from_slice(match p.line_ending {
        LineEndingParam::Lf => b"\n",
        LineEndingParam::Crlf => b"\r\n",
        LineEndingParam::None => b"",
    });
    Ok(SendRequest {
        bytes,
        expect: p.expect.clone(),
    })
}

/// `{ ok, data, error, warnings }`: the CLI envelope's payload, marked as a tool error
/// when `error` is set.
pub fn to_result<T: Serialize>(r: OpResult<T>) -> CallToolResult {
    let mut data = r.data_json();
    let mut warnings = r.warnings;
    if let Some(d) = data.as_mut() {
        warnings.extend(pogo_core::cap::apply(d, pogo_core::cap::SOFT_CAP_BYTES));
    }
    let body = serde_json::json!({
        "ok": r.error.is_none(),
        "data": data,
        "error": r.error,
        "warnings": warnings,
    });
    if r.error.is_some() {
        CallToolResult::structured_error(body)
    } else {
        CallToolResult::structured(body)
    }
}

pub fn fail(e: ErrorBody) -> CallToolResult {
    to_result::<()>(OpResult::fail(e))
}

/// Core ops block on serial I/O, so they run off the async runtime.
pub async fn blocking<T, F>(f: F) -> CallToolResult
where
    F: FnOnce() -> OpResult<T> + Send + 'static,
    T: Serialize + Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(r) => to_result(r),
        Err(e) => fail(ErrorBody::new(
            ErrorCode::Internal,
            format!("tool task failed: {e}"),
        )),
    }
}
