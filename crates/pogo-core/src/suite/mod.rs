//! Hardware test suites for `pogo test` (docs/commands.md §7.14): a TOML file of named
//! tests, each a list of steps run in order on one locked board.

pub mod junit;

use std::path::{Path, PathBuf};
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::discovery::ResolvedDevice;
use crate::flash::FlashReport;

pub const DEFAULT_RESET_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_SEND_TIMEOUT: Duration = Duration::from_secs(5);
pub const DEFAULT_WAIT_TIMEOUT: Duration = Duration::from_secs(10);

/// The file as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSuite {
    firmware: Option<PathBuf>,
    #[serde(default, rename = "test")]
    tests: Vec<RawTest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTest {
    name: String,
    #[serde(default)]
    steps: Vec<RawStep>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStep {
    reset: Option<bool>,
    send: Option<String>,
    wait: Option<String>,
    until: Option<String>,
    expect: Option<String>,
    timeout: Option<String>,
    within: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Reset, then capture from the first boot byte until `until` (or the timeout).
    Reset {
        until: Option<String>,
        timeout: Duration,
        /// `until` must match this soon after the capture starts.
        within: Option<Duration>,
    },
    /// Send a line, then capture until `expect` if given.
    Send {
        data: String,
        expect: Option<String>,
        timeout: Duration,
    },
    /// Capture without sending until a line matches.
    Wait { pattern: String, timeout: Duration },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCase {
    pub name: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suite {
    /// Resolved against the suite file's directory.
    pub firmware: Option<PathBuf>,
    pub tests: Vec<TestCase>,
}

fn duration(s: &Option<String>, default: Duration, at: &str) -> Result<Duration, String> {
    match s {
        None => Ok(default),
        Some(s) => {
            humantime::parse_duration(s).map_err(|e| format!("{at}: invalid duration '{s}': {e}"))
        }
    }
}

fn step(raw: &RawStep, at: &str) -> Result<Step, String> {
    let kinds = [raw.reset.is_some(), raw.send.is_some(), raw.wait.is_some()];
    if kinds.iter().filter(|k| **k).count() != 1 {
        return Err(format!("{at}: give exactly one of reset, send, or wait"));
    }
    let only = |ok: bool, key: &str, with: &str| {
        if ok {
            Ok(())
        } else {
            Err(format!("{at}: `{key}` only goes with `{with}`"))
        }
    };
    only(raw.until.is_none() || raw.reset.is_some(), "until", "reset")?;
    only(raw.expect.is_none() || raw.send.is_some(), "expect", "send")?;
    only(
        raw.within.is_none() || raw.until.is_some(),
        "within",
        "reset and until",
    )?;
    if raw.reset == Some(false) {
        return Err(format!(
            "{at}: `reset = false` does nothing; use `wait` to capture"
        ));
    }
    for pattern in [&raw.until, &raw.expect, &raw.wait].into_iter().flatten() {
        regex::Regex::new(pattern)
            .map_err(|e| format!("{at}: invalid pattern '{pattern}': {e}"))?;
    }
    Ok(if let Some(data) = &raw.send {
        Step::Send {
            data: data.clone(),
            expect: raw.expect.clone(),
            timeout: duration(&raw.timeout, DEFAULT_SEND_TIMEOUT, at)?,
        }
    } else if let Some(pattern) = &raw.wait {
        Step::Wait {
            pattern: pattern.clone(),
            timeout: duration(&raw.timeout, DEFAULT_WAIT_TIMEOUT, at)?,
        }
    } else {
        Step::Reset {
            until: raw.until.clone(),
            timeout: duration(&raw.timeout, DEFAULT_RESET_TIMEOUT, at)?,
            within: raw
                .within
                .as_ref()
                .map(|w| duration(&Some(w.clone()), Duration::ZERO, at))
                .transpose()?,
        }
    })
}

/// Parse and check a suite. Errors name the test and step, for a `USAGE` error.
pub fn parse(text: &str, dir: &Path) -> Result<Suite, String> {
    let raw: RawSuite = toml::from_str(text).map_err(|e| e.to_string())?;
    if raw.tests.is_empty() {
        return Err("no tests: add at least one [[test]] table".into());
    }
    let mut tests = Vec::new();
    for t in &raw.tests {
        if t.steps.is_empty() {
            return Err(format!("test '{}': no steps", t.name));
        }
        if tests.iter().any(|x: &TestCase| x.name == t.name) {
            return Err(format!("test '{}' is defined twice", t.name));
        }
        let steps = t
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| step(s, &format!("test '{}' step {}", t.name, i + 1)))
            .collect::<Result<_, _>>()?;
        tests.push(TestCase {
            name: t.name.clone(),
            steps,
        });
    }
    Ok(Suite {
        firmware: raw.firmware.map(|f| dir.join(f)),
        tests,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Failure {
    /// 1-based step number.
    pub step: usize,
    pub message: String,
    /// The last lines captured in the failing step, oldest first.
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TestResult {
    pub name: String,
    pub status: Status,
    pub duration_ms: u64,
    pub failure: Option<Failure>,
}

/// `data` for `pogo test`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SuiteData {
    pub file: String,
    pub device: ResolvedDevice,
    /// Set when the suite's firmware was flashed first.
    pub flash: Option<FlashReport>,
    pub tests: Vec<TestResult>,
    pub passed: usize,
    pub failed: usize,
    /// Where the JUnit report was written, with `--junit`.
    pub junit_path: Option<String>,
}

#[cfg(test)]
mod tests;
