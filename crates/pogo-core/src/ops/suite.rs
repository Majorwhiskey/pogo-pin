//! `pogo test`: run a suite file on one locked board.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use super::OpResult;
use super::capture::{CaptureRequest, Live, capture_locked};
use super::flash::{DEFAULT_FLASH_TIMEOUT, FlashRequest, flash_target, prepare};
use super::outcome::outcome_error;
use super::send::{SendRequest, send_locked};
use super::target::{Selection, Target, acquire};
use super::usage;
use crate::envelope::{ErrorBody, Warning};
use crate::error::ErrorCode;
use crate::serial::result::{CaptureResult, Outcome};
use crate::suite::{self, Failure, Status, Step, SuiteData, TestCase, TestResult, junit};

/// Lines of context kept from a failing step.
const FAILURE_LINES: usize = 20;

#[derive(Debug, Clone, Default)]
pub struct SuiteRequest {
    pub file: PathBuf,
    /// Skip flashing the suite's `firmware`.
    pub no_flash: bool,
    /// Stop after the first failing test.
    pub fail_fast: bool,
    /// Also write a JUnit XML report here.
    pub junit: Option<PathBuf>,
}

pub fn run_suite(
    sel: &Selection,
    req: &SuiteRequest,
    cancel: Option<Arc<AtomicBool>>,
) -> OpResult<SuiteData> {
    let text = match std::fs::read_to_string(&req.file) {
        Ok(t) => t,
        Err(e) => return OpResult::fail(usage(format!("cannot read {}: {e}", req.file.display()))),
    };
    let dir = req.file.parent().unwrap_or(Path::new("."));
    let suite = match suite::parse(&text, dir) {
        Ok(s) => s,
        Err(e) => return OpResult::fail(usage(format!("{}: {e}", req.file.display()))),
    };
    let target = match acquire(sel, "test") {
        Ok(t) => t,
        Err(e) => return OpResult::fail(e),
    };
    let mut warnings = Vec::new();
    let flash = match (&suite.firmware, req.no_flash) {
        (Some(image), false) => {
            let freq = FlashRequest {
                image: image.clone(),
                verify: true,
                ..FlashRequest::default()
            };
            let flashed = prepare(&freq, true).and_then(|p| {
                flash_target(&target, &p, false, DEFAULT_FLASH_TIMEOUT, cancel.as_ref())
            });
            match flashed {
                Ok((data, w)) => {
                    warnings.extend(w);
                    data.report
                }
                Err(e) => return OpResult::fail(e).with_warnings(warnings),
            }
        }
        _ => None,
    };

    let elf = suite.firmware.clone();
    let mut tests = Vec::new();
    for case in &suite.tests {
        let result = run_case(sel, &target, case, elf.as_deref(), &cancel, &mut warnings);
        let failed = result.status == Status::Failed;
        tests.push(result);
        if failed && req.fail_fast {
            break;
        }
    }
    let failed = tests.iter().filter(|t| t.status == Status::Failed).count();
    let mut data = SuiteData {
        file: req.file.display().to_string(),
        device: target.device.clone(),
        flash,
        passed: tests.len() - failed,
        failed,
        tests,
        junit_path: None,
    };
    if let Some(path) = &req.junit {
        match std::fs::write(path, junit::render(&data)) {
            Ok(()) => data.junit_path = Some(path.display().to_string()),
            Err(e) => warnings.push(Warning::new(
                "JUNIT_FAILED",
                format!("cannot write {}: {e}", path.display()),
            )),
        }
    }
    let error = (failed > 0).then(|| {
        let names: Vec<&str> = data
            .tests
            .iter()
            .filter(|t| t.status == Status::Failed)
            .map(|t| t.name.as_str())
            .collect();
        ErrorBody::new(
            ErrorCode::CheckFailed,
            format!(
                "{failed} of {} tests failed: {}",
                data.tests.len(),
                names.join(", ")
            ),
        )
        .with_hint("Each failed test's step, message, and last lines are in data.tests.")
    });
    OpResult::ok(data).with_warnings(warnings).with_error(error)
}

fn run_case(
    sel: &Selection,
    target: &Target,
    case: &TestCase,
    elf: Option<&Path>,
    cancel: &Option<Arc<AtomicBool>>,
    warnings: &mut Vec<Warning>,
) -> TestResult {
    let start = Instant::now();
    let mut failure = None;
    for (i, step) in case.steps.iter().enumerate() {
        let live = Live {
            on_line: &mut |_| {},
            cancel: cancel.clone(),
        };
        if let Err((message, lines)) = run_step(sel, target, step, elf, live, warnings) {
            failure = Some(Failure {
                step: i + 1,
                message,
                lines,
            });
            break;
        }
    }
    TestResult {
        name: case.name.clone(),
        status: if failure.is_some() {
            Status::Failed
        } else {
            Status::Passed
        },
        duration_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
        failure,
    }
}

type StepFailure = (String, Vec<String>);

fn tail(c: &CaptureResult) -> Vec<String> {
    let skip = c.lines.len().saturating_sub(FAILURE_LINES);
    c.lines[skip..].iter().map(|l| l.text.clone()).collect()
}

/// A capture passes when its pattern matched (or, with no pattern, it ran to the end).
fn judge(
    c: &CaptureResult,
    until: &[String],
    timeout: std::time::Duration,
) -> Result<(), StepFailure> {
    match outcome_error(c, until, timeout, None) {
        Some(e) => Err((e.message, tail(c))),
        None if c.outcome == Outcome::Matched || until.is_empty() => Ok(()),
        None => Err((
            format!("capture ended with outcome {:?}", c.outcome),
            tail(c),
        )),
    }
}

fn run_step(
    sel: &Selection,
    target: &Target,
    step: &Step,
    elf: Option<&Path>,
    live: Live,
    warnings: &mut Vec<Warning>,
) -> Result<(), StepFailure> {
    let capture = |until: &Option<String>, reset: bool, timeout, live: Live| {
        let req = CaptureRequest {
            until: until.iter().cloned().collect(),
            elf: elf.map(Path::to_path_buf),
            ..CaptureRequest::default()
        };
        capture_locked(sel, target, &req, reset, timeout, live)
            .map(|done| (done.out.data.capture, done.out.warnings))
            .map_err(|e| (e.message, Vec::new()))
    };
    match step {
        Step::Reset {
            until,
            timeout,
            within,
        } => {
            let (c, w) = capture(until, true, *timeout, live)?;
            warnings.extend(w);
            judge(&c, &until.iter().cloned().collect::<Vec<_>>(), *timeout)?;
            match (within, &c.matched) {
                (Some(limit), Some(m)) => {
                    check_within(m.t_ms, *limit).map_err(|msg| (msg, tail(&c)))
                }
                _ => Ok(()),
            }
        }
        Step::Wait { pattern, timeout } => {
            let until = Some(pattern.clone());
            let (c, w) = capture(&until, false, *timeout, live)?;
            warnings.extend(w);
            judge(&c, std::slice::from_ref(pattern), *timeout)
        }
        Step::Send {
            data,
            expect,
            timeout,
        } => {
            let req = SendRequest {
                bytes: format!("{data}\n").into_bytes(),
                expect: expect.clone(),
                elf: elf.map(Path::to_path_buf),
            };
            let (sent, w) = send_locked(target, &req, *timeout, live).map_err(|(e, w)| {
                warnings.extend(w);
                (e.message, Vec::new())
            })?;
            warnings.extend(w);
            match &sent.response {
                Some(c) => judge(c, &expect.iter().cloned().collect::<Vec<_>>(), *timeout),
                None => Ok(()),
            }
        }
    }
}

/// `within`: the match must come no later than `limit` after the capture starts.
fn check_within(t_ms: u64, limit: std::time::Duration) -> Result<(), String> {
    if u128::from(t_ms) > limit.as_millis() {
        return Err(format!(
            "matched after {t_ms} ms, later than the {} ms allowed",
            limit.as_millis()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::check_within;

    #[test]
    fn within_allows_up_to_the_limit() {
        assert!(check_within(500, Duration::from_millis(500)).is_ok());
        let e = check_within(501, Duration::from_millis(500)).unwrap_err();
        assert_eq!(e, "matched after 501 ms, later than the 500 ms allowed");
    }
}
