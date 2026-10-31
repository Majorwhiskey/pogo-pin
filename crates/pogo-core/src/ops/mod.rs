//! Whole commands as core functions: device selection, locking, guardrails, the hardware
//! work, and the mapping of outcomes to errors. The CLI and the MCP server are thin
//! adapters over these, so both behave the same (docs/mcp.md §9).

pub mod capture;
pub mod devices;
pub mod doctor;
pub mod flash;
pub mod outcome;
pub mod reset;
pub mod run;
pub mod send;
pub mod target;

use std::time::Duration;

use serde::Serialize;

use crate::envelope::{ErrorBody, Warning};
use crate::error::{ErrorCode, PogoError};

pub use target::{Selection, Target};

/// A finished command: `data` may be present even with an `error` (partial results).
#[derive(Debug, Clone)]
pub struct OpResult<T> {
    pub data: Option<T>,
    pub error: Option<ErrorBody>,
    pub warnings: Vec<Warning>,
}

impl<T> OpResult<T> {
    pub fn ok(data: T) -> Self {
        OpResult {
            data: Some(data),
            error: None,
            warnings: Vec::new(),
        }
    }

    pub fn fail(error: impl Into<ErrorBody>) -> Self {
        OpResult {
            data: None,
            error: Some(error.into()),
            warnings: Vec::new(),
        }
    }

    pub fn with_error(mut self, error: Option<ErrorBody>) -> Self {
        self.error = error;
        self
    }

    pub fn with_warnings(mut self, warnings: impl IntoIterator<Item = Warning>) -> Self {
        self.warnings.extend(warnings);
        self
    }
}

impl<T: Serialize> OpResult<T> {
    /// `data` as JSON, for adapters that do not know the concrete type.
    pub fn data_json(&self) -> Option<serde_json::Value> {
        self.data
            .as_ref()
            .and_then(|d| serde_json::to_value(d).ok())
    }
}

impl<T> From<PogoError> for OpResult<T> {
    fn from(e: PogoError) -> Self {
        OpResult::fail(&e)
    }
}

pub(crate) fn usage(msg: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorCode::Usage, msg)
}

pub const DEFAULT_CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_SEND_TIMEOUT: Duration = Duration::from_secs(5);
