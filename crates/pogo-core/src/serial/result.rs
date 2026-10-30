//! Capture result types: the `data` of `serial capture` (docs/commands.md §7.6).

use schemars::JsonSchema;
use serde::Serialize;

use super::LogLine;
use super::detect::{PanicReport, ResetEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Matched,
    Completed,
    Fault,
    Timeout,
    Eof,
    /// Stopped by Ctrl-C or SIGTERM; the data is partial.
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PatternMatch {
    pub pattern: String,
    pub line_index: usize,
    pub t_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FaultKind {
    FailOn,
    Panic,
    UnexpectedReboot,
}

/// What turned the outcome into `fault`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Fault {
    pub kind: FaultKind,
    /// The fail-on pattern that matched, if any.
    pub pattern: Option<String>,
    pub line_index: usize,
    pub t_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CaptureResult {
    pub outcome: Outcome,
    pub matched: Option<PatternMatch>,
    pub fault: Option<Fault>,
    /// Lines seen, including omitted ones. Line indexes count from 0 over all of them.
    pub line_count: usize,
    pub omitted_lines: usize,
    pub boot_count: u32,
    pub resets: Vec<ResetEvent>,
    pub panic: Option<PanicReport>,
    pub lines: Vec<LogLine>,
}
