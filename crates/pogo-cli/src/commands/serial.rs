//! `pogo serial capture`: clap flags to a core `CaptureRequest`, and output rendering.

use std::io::Write;

use pogo_core::config::ParserName;
use pogo_core::ops::capture::{CaptureRequest, Live, serial_capture};
use pogo_core::serial::LogLine;
use pogo_core::serial::session::SerialCaptureData;

use crate::cli::{CaptureArgs, GlobalArgs, LogParser};
use crate::commands::target::{reply, selection};
use crate::interrupt::interrupt_flag;
use crate::output::{Format, Render, Reply, Style};

pub fn capture(
    global: &GlobalArgs,
    reset: bool,
    args: &CaptureArgs,
    format: Format,
    style: &Style,
) -> Reply {
    let mut stream = |line: &LogLine| stream_line(format, line);
    let result = serial_capture(
        &selection(global),
        &capture_request(args),
        reset,
        global.timeout,
        live(&mut stream),
    );
    reply(result, style)
}

pub fn capture_request(args: &CaptureArgs) -> CaptureRequest {
    CaptureRequest {
        baud: args.baud,
        until: args.until.clone(),
        fail_on: args.fail_on.clone(),
        no_default_fail_patterns: args.no_default_fail_patterns,
        settle: args.settle,
        max_lines: args.max_lines,
        max_line_len: args.max_line_len,
        parser: args.parser.map(parser_name),
        elf: args.elf.clone(),
        raw: args.raw,
        record: args.record.clone(),
        reconnect_timeout: args.reconnect_timeout,
    }
}

/// Lines stream to stdout and Ctrl-C stops the capture with partial results.
pub fn live<'a>(on_line: &'a mut dyn FnMut(&LogLine)) -> Live<'a> {
    Live {
        on_line,
        cancel: Some(interrupt_flag()),
    }
}

/// Lines go out as they arrive: plain text in human mode, `line` events in jsonl.
pub fn stream_line(format: Format, line: &LogLine) {
    let text = match format {
        Format::Human => format!("{}\n", line.text),
        Format::Jsonl => {
            let mut v = serde_json::to_value(line).unwrap_or_default();
            if let Some(obj) = v.as_object_mut() {
                obj.insert("type".into(), "line".into());
            }
            format!("{v}\n")
        }
        Format::Json => return,
    };
    let _ = std::io::stdout().lock().write_all(text.as_bytes());
}

fn parser_name(p: LogParser) -> ParserName {
    match p {
        LogParser::Auto => ParserName::Auto,
        LogParser::EspIdf => ParserName::EspIdf,
        LogParser::RustLog => ParserName::RustLog,
        LogParser::Raw => ParserName::Raw,
    }
}

impl Render for SerialCaptureData {
    /// Lines were already streamed; this is the closing summary.
    fn render_human(&self, out: &mut String, style: &Style) {
        let c = &self.capture;
        let summary = match &c.matched {
            Some(m) => format!(
                "matched /{}/ on line {} after {} ms",
                m.pattern, m.line_index, m.t_ms
            ),
            None => format!("captured {} lines", c.line_count),
        };
        out.push_str(&style.dim(&format!("-- {summary} ({})", self.device.port)));
        out.push('\n');
    }
}
