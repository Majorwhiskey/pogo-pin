//! The JSON result envelope shared by every command (README §6.3).

use schemars::JsonSchema;
use serde::Serialize;

use crate::error::{ErrorCode, PogoError};

/// Version of the envelope and all `data` shapes. Additive changes only within a version.
pub const SCHEMA_VERSION: u32 = 1;

/// Tool version reported in every envelope.
pub const POGO_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Envelope<T> {
    pub schema_version: u32,
    pub pogo_version: String,
    /// Dotted command name, e.g. `serial.capture`.
    pub command: String,
    /// `true` only when the exit code is 0.
    pub ok: bool,
    pub duration_ms: u64,
    /// Command-specific result. May be present even when `ok` is false (partial results).
    pub data: Option<T>,
    pub error: Option<ErrorBody>,
    pub warnings: Vec<Warning>,
}

impl<T> Envelope<T> {
    pub fn new(
        command: impl Into<String>,
        duration_ms: u64,
        data: Option<T>,
        error: Option<ErrorBody>,
        warnings: Vec<Warning>,
    ) -> Self {
        Envelope {
            schema_version: SCHEMA_VERSION,
            pogo_version: POGO_VERSION.to_string(),
            command: command.into(),
            ok: error.is_none(),
            duration_ms,
            data,
            error,
            warnings,
        }
    }

    /// Process exit code implied by this envelope.
    pub fn exit_code(&self) -> u8 {
        self.error.as_ref().map_or(0, |e| e.code.exit_code())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    pub hint: Option<String>,
    pub details: Option<serde_json::Value>,
}

impl ErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        ErrorBody {
            code,
            message: message.into(),
            hint: None,
            details: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl From<&PogoError> for ErrorBody {
    fn from(err: &PogoError) -> Self {
        ErrorBody {
            code: err.code(),
            message: err.to_string(),
            hint: err.hint(),
            details: err.details(),
        }
    }
}

/// Something worth knowing that did not cause the command to fail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Warning {
    /// Stable `SCREAMING_SNAKE_CASE` code, e.g. `OUTPUT_TRUNCATED`.
    pub code: String,
    pub message: String,
}

impl Warning {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Warning {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_tracks_error() {
        let ok = Envelope::new("devices", 3, Some(1u8), None, vec![]);
        assert!(ok.ok);
        assert_eq!(ok.exit_code(), 0);

        let err = PogoError::NotImplemented("flash");
        let failed = Envelope::<u8>::new("flash", 3, None, Some((&err).into()), vec![]);
        assert!(!failed.ok);
        assert_eq!(failed.exit_code(), 1);
    }

    #[test]
    fn field_order_matches_spec() {
        let env = Envelope::new("devices", 3, Some(1u8), None, vec![]);
        let json = serde_json::to_string(&env).unwrap();
        let keys = [
            "schema_version",
            "pogo_version",
            "command",
            "ok",
            "duration_ms",
            "data",
            "error",
            "warnings",
        ];
        let positions: Vec<usize> = keys
            .iter()
            .map(|k| json.find(&format!("\"{k}\"")).unwrap())
            .collect();
        assert!(positions.is_sorted(), "{json}");
    }
}
