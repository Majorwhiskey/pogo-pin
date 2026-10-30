//! Audit log (docs/guardrails.md §11): every action that changes a device is appended as
//! one JSON line, so a human can see what an agent did to their hardware.

use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AuditEntry<'a> {
    pub ts_unix_ms: u64,
    pub command: &'a str,
    pub port: &'a str,
    pub usb_serial: Option<&'a str>,
    pub args: serde_json::Value,
    /// `ok`, or the error code.
    pub result: &'a str,
}

impl<'a> AuditEntry<'a> {
    pub fn new(
        command: &'a str,
        port: &'a str,
        usb_serial: Option<&'a str>,
        args: serde_json::Value,
        result: &'a str,
    ) -> Self {
        let ts_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0);
        AuditEntry {
            ts_unix_ms,
            command,
            port,
            usb_serial,
            args,
            result,
        }
    }
}

/// `~/.local/state/pogo/audit.jsonl` on Linux; the local data directory elsewhere.
pub fn audit_path() -> Option<PathBuf> {
    let base = directories::BaseDirs::new()?;
    let dir = base
        .state_dir()
        .unwrap_or_else(|| base.data_local_dir())
        .join("pogo");
    Some(dir.join("audit.jsonl"))
}

/// Append one entry. Callers turn a failure into a warning; it never blocks the action.
pub fn record(entry: &AuditEntry) -> Result<(), String> {
    let path = audit_path().ok_or("no home directory for the audit log")?;
    append(&path, entry)
}

pub fn append(path: &std::path::Path, entry: &AuditEntry) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("cannot write audit log {}: {e}", path.display());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(fail)?;
    }
    let line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(fail)?;
    writeln!(file, "{line}").map_err(fail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_append_as_json_lines() {
        let path = std::env::temp_dir()
            .join(format!("pogo-audit-{}", std::process::id()))
            .join("audit.jsonl");
        let args = serde_json::json!({ "image": "app.elf" });
        append(
            &path,
            &AuditEntry::new("flash", "/dev/x", Some("AA"), args, "ok"),
        )
        .unwrap();
        append(
            &path,
            &AuditEntry::new("reset", "/dev/x", None, serde_json::json!({}), "DEVICE_IO"),
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["command"], "flash");
        assert_eq!(lines[0]["usb_serial"], "AA");
        assert_eq!(lines[1]["result"], "DEVICE_IO");
        assert!(lines[1]["ts_unix_ms"].as_u64().unwrap() > 1_700_000_000_000);
    }
}
