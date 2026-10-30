//! The capture engine (docs/commands.md §7.6): read lines until an `--until` pattern
//! matches, a fault is detected, or the deadline passes. Outcomes are data, not errors.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use regex::Regex;

use super::detect::{
    BlockEnd, ResetEvent, is_panic_marker, panic_block_end, panic_report, parse_reset,
};
pub use super::result::{CaptureResult, Fault, FaultKind, Outcome, PatternMatch};
use super::{DecodeOptions, LineSplitter, LogLine, RawLine, decode_line, strip_ansi};
use crate::deadline::Deadline;
use crate::error::PogoError;
use crate::transport::Transport;

/// How long to keep reading after a panic marker to collect the rest of the block.
const PANIC_COLLECT: Duration = Duration::from_secs(2);
const PANIC_BLOCK_MAX_LINES: usize = 200;
const READ_SLICE: Duration = Duration::from_millis(100);

#[derive(Debug, Clone)]
pub struct CaptureOptions {
    pub until: Vec<Regex>,
    /// Checked before `until`. Callers merge defaults, config, and flags.
    pub fail_on: Vec<Regex>,
    /// After an `until` match, keep reading this long; a fault in the window wins.
    pub settle: Duration,
    /// Lines kept in the result; the excess is dropped from the middle (20% head, 80% tail).
    pub max_lines: usize,
    pub decode: DecodeOptions,
    /// The capture started with a reset, so any boot after the first is a fault.
    pub expect_one_boot: bool,
    /// Set from another thread (Ctrl-C) to stop early with outcome `interrupted`.
    pub cancel: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Running,
    Settling(Deadline),
    Collecting(Deadline),
    Done,
}

/// `on_line` sees every decoded line as it arrives, for streaming output.
pub fn capture(
    t: &mut dyn Transport,
    opts: &CaptureOptions,
    deadline: Deadline,
    on_line: &mut dyn FnMut(&LogLine),
) -> Result<CaptureResult, PogoError> {
    let mut e = Engine::new(opts, deadline, on_line);
    let mut splitter = LineSplitter::new(opts.decode.max_line_bytes());
    let mut buf = [0u8; 1024];
    let mut eof = false;
    let mut interrupted = false;
    while e.phase != Phase::Done {
        if opts
            .cancel
            .as_ref()
            .is_some_and(|c| c.load(Ordering::Relaxed))
        {
            interrupted = true;
            break;
        }
        let wait = match e.phase {
            Phase::Settling(d) | Phase::Collecting(d) => d,
            Phase::Running | Phase::Done => deadline,
        };
        let Some(remaining) = wait.remaining() else {
            break;
        };
        match t.read(&mut buf, remaining.min(READ_SLICE)) {
            Ok(0) => {}
            Ok(n) => {
                let t_ms = e.t_ms();
                for raw in splitter.push(&buf[..n], t_ms) {
                    e.line(raw);
                    if e.phase == Phase::Done {
                        break;
                    }
                }
            }
            Err(PogoError::DeviceIo(_)) => {
                eof = true;
                break;
            }
            Err(err) => return Err(err),
        }
    }
    if e.phase != Phase::Done
        && let Some(raw) = splitter.finish()
    {
        e.line(raw);
    }
    Ok(e.finish(eof, interrupted))
}

struct Engine<'a> {
    opts: &'a CaptureOptions,
    on_line: &'a mut dyn FnMut(&LogLine),
    deadline: Deadline,
    start: Instant,
    phase: Phase,
    line_count: usize,
    head: Vec<LogLine>,
    tail: VecDeque<LogLine>,
    boot_count: u32,
    resets: Vec<ResetEvent>,
    matched: Option<PatternMatch>,
    fault: Option<Fault>,
    panic_block: Option<(usize, Vec<String>)>,
}

impl<'a> Engine<'a> {
    fn new(
        opts: &'a CaptureOptions,
        deadline: Deadline,
        on_line: &'a mut dyn FnMut(&LogLine),
    ) -> Self {
        Engine {
            opts,
            on_line,
            deadline,
            start: Instant::now(),
            phase: Phase::Running,
            line_count: 0,
            head: Vec::new(),
            tail: VecDeque::new(),
            boot_count: 0,
            resets: Vec::new(),
            matched: None,
            fault: None,
            panic_block: None,
        }
    }

    fn t_ms(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn line(&mut self, raw: RawLine) {
        let line = decode_line(raw, &self.opts.decode);
        let index = self.line_count;
        self.line_count += 1;
        let clean = if self.opts.decode.keep_ansi {
            strip_ansi(&line.text)
        } else {
            line.text.clone()
        };
        let t_ms = line.t_ms;
        (self.on_line)(&line);
        self.keep(line);

        if let Phase::Collecting(_) = self.phase {
            let end = panic_block_end(&clean);
            if end != BlockEnd::Before
                && let Some((_, block)) = &mut self.panic_block
                && block.len() < PANIC_BLOCK_MAX_LINES
            {
                block.push(clean);
            }
            if end != BlockEnd::Inside {
                self.phase = Phase::Done;
            }
            return;
        }

        if let Some(reset) = parse_reset(&clean, t_ms) {
            self.boot_count += 1;
            self.resets.push(reset);
            if self.opts.expect_one_boot && self.boot_count > 1 && self.fault.is_none() {
                self.fault(FaultKind::UnexpectedReboot, None, index, t_ms);
                self.phase = Phase::Done;
                return;
            }
        }

        if self.fault.is_none()
            && let Some(re) = self.opts.fail_on.iter().find(|re| re.is_match(&clean))
        {
            let pattern = Some(re.as_str().to_string());
            if is_panic_marker(&clean) {
                self.fault(FaultKind::Panic, pattern, index, t_ms);
                self.panic_block = Some((index, vec![clean]));
                self.phase = Phase::Collecting(self.deadline.min_after(PANIC_COLLECT));
            } else {
                self.fault(FaultKind::FailOn, pattern, index, t_ms);
                self.phase = Phase::Done;
            }
            return;
        }

        if self.phase == Phase::Running
            && let Some(re) = self.opts.until.iter().find(|re| re.is_match(&clean))
        {
            self.matched = Some(PatternMatch {
                pattern: re.as_str().to_string(),
                line_index: index,
                t_ms,
            });
            self.phase = if self.opts.settle.is_zero() {
                Phase::Done
            } else {
                Phase::Settling(self.deadline.min_after(self.opts.settle))
            };
        }
    }

    fn fault(&mut self, kind: FaultKind, pattern: Option<String>, line_index: usize, t_ms: u64) {
        self.fault = Some(Fault {
            kind,
            pattern,
            line_index,
            t_ms,
        });
    }

    /// Keep the first 20% and the most recent 80% of `max_lines` (docs/agent-contract.md
    /// §6.5), so both the boot output and the end of a crash survive.
    fn keep(&mut self, line: LogLine) {
        let head_cap = self.opts.max_lines / 5;
        let tail_cap = self.opts.max_lines - head_cap;
        if self.head.len() < head_cap {
            self.head.push(line);
        } else if tail_cap > 0 {
            if self.tail.len() == tail_cap {
                self.tail.pop_front();
            }
            self.tail.push_back(line);
        }
    }

    fn finish(self, eof: bool, interrupted: bool) -> CaptureResult {
        let outcome = if interrupted {
            Outcome::Interrupted
        } else if self.fault.is_some() {
            Outcome::Fault
        } else if eof {
            Outcome::Eof
        } else if self.matched.is_some() {
            Outcome::Matched
        } else if self.opts.until.is_empty() {
            Outcome::Completed
        } else {
            Outcome::Timeout
        };
        let panic = self.panic_block.map(|(index, block)| {
            let refs: Vec<&str> = block.iter().map(String::as_str).collect();
            panic_report(index, &refs)
        });
        let mut lines = self.head;
        lines.extend(self.tail);
        CaptureResult {
            outcome,
            matched: self.matched,
            fault: self.fault,
            line_count: self.line_count,
            omitted_lines: self.line_count - lines.len(),
            boot_count: self.boot_count,
            resets: self.resets,
            panic,
            lines,
        }
    }
}
