//! Output formatting and stream discipline (docs/agent-contract.md §6.1–6.2).
//!
//! stdout carries results only; stderr carries errors, warnings, and logs in human mode.

use std::io::{IsTerminal, Write};

use clap::ValueEnum;
use pogo_core::{Envelope, ErrorBody};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Human,
    Json,
    Jsonl,
}

/// A command's result, with its JSON form and human rendering computed up front.
pub struct Reply {
    pub data: Option<serde_json::Value>,
    pub human: String,
    pub error: Option<ErrorBody>,
    pub warnings: Vec<pogo_core::Warning>,
}

impl Reply {
    pub fn ok<T: Serialize + Render>(data: &T, style: &Style) -> Self {
        let mut human = String::new();
        data.render_human(&mut human, style);
        Reply {
            data: Some(to_value(data)),
            human,
            error: None,
            warnings: Vec::new(),
        }
    }

    /// Keep the data (partial results) and mark the command as failed.
    pub fn with_error(mut self, error: impl Into<ErrorBody>) -> Self {
        self.error = Some(error.into());
        self
    }

    pub fn error(error: impl Into<ErrorBody>) -> Self {
        Reply {
            data: None,
            human: String::new(),
            error: Some(error.into()),
            warnings: Vec::new(),
        }
    }
}

fn to_value<T: Serialize>(data: &T) -> serde_json::Value {
    // Result types are plain structs; serialization cannot fail short of a bug.
    serde_json::to_value(data)
        .unwrap_or_else(|e| serde_json::json!({ "serialization_error": e.to_string() }))
}

/// Human-readable rendering for a command's `data`.
pub trait Render {
    fn render_human(&self, out: &mut String, style: &Style);
}

/// ANSI styling that collapses to plain text when color is off.
#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub color: bool,
}

impl Style {
    /// Colors appear only in human mode, on a TTY, without `--no-color` or `NO_COLOR`.
    pub fn detect(format: Format, no_color: bool) -> Self {
        let no_color_env = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        Style {
            color: format == Format::Human
                && !no_color
                && !no_color_env
                && std::io::stdout().is_terminal(),
        }
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn bold(&self, text: &str) -> String {
        self.paint("1", text)
    }
    pub fn dim(&self, text: &str) -> String {
        self.paint("2", text)
    }
    pub fn red(&self, text: &str) -> String {
        self.paint("31", text)
    }
    pub fn green(&self, text: &str) -> String {
        self.paint("32", text)
    }
    pub fn yellow(&self, text: &str) -> String {
        self.paint("33", text)
    }
}

#[derive(Serialize)]
struct JsonlResult<'a, T> {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(flatten)]
    envelope: &'a Envelope<T>,
}

pub struct Printer {
    pub format: Format,
    pub style: Style,
    pub quiet: bool,
}

impl Printer {
    /// Print the final result and return the process exit code.
    pub fn finish(&self, command: &str, duration_ms: u64, reply: Reply) -> u8 {
        let envelope = Envelope::new(
            command,
            duration_ms,
            reply.data,
            reply.error,
            reply.warnings,
        );
        let (stdout, stderr) = self.render(&envelope, &reply.human);
        // Write failures (e.g. a closed pipe) must not change the exit code.
        if !stdout.is_empty() {
            let _ = std::io::stdout().lock().write_all(stdout.as_bytes());
        }
        if !stderr.is_empty() {
            let _ = std::io::stderr().lock().write_all(stderr.as_bytes());
        }
        envelope.exit_code()
    }

    /// Returns (stdout, stderr) text for an envelope.
    pub fn render(&self, envelope: &Envelope<serde_json::Value>, human: &str) -> (String, String) {
        match self.format {
            Format::Json => (json_line(envelope), String::new()),
            Format::Jsonl => (
                json_line(&JsonlResult {
                    kind: "result",
                    envelope,
                }),
                String::new(),
            ),
            Format::Human => {
                let mut stdout = human.to_string();
                if !stdout.is_empty() && !stdout.ends_with('\n') {
                    stdout.push('\n');
                }
                let mut stderr = String::new();
                if !self.quiet {
                    for w in &envelope.warnings {
                        let label = format!("warning[{}]", w.code);
                        stderr.push_str(&format!("{}: {}\n", self.style.yellow(&label), w.message));
                    }
                }
                if let Some(err) = &envelope.error {
                    let label = format!("error[{}]", err.code);
                    stderr.push_str(&format!("{}: {}\n", self.style.red(&label), err.message));
                    if let Some(hint) = &err.hint {
                        stderr.push_str(&format!("  {} {hint}\n", self.style.dim("hint:")));
                    }
                }
                (stdout, stderr)
            }
        }
    }
}

fn json_line<T: Serialize>(value: &T) -> String {
    let mut s = serde_json::to_string(value)
        .unwrap_or_else(|e| format!("{{\"serialization_error\":{:?}}}", e.to_string()));
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use pogo_core::{PogoError, Warning};

    fn printer(format: Format) -> Printer {
        Printer {
            format,
            style: Style { color: false },
            quiet: false,
        }
    }

    fn failed() -> Envelope<serde_json::Value> {
        let err = PogoError::DeviceNotFound {
            selector: "target".into(),
        };
        Envelope::new(
            "flash",
            12,
            None,
            Some((&err).into()),
            vec![Warning::new("NO_USB_SERIAL", "port path used as identity")],
        )
    }

    #[test]
    fn json_is_one_document_on_stdout_only() {
        let (stdout, stderr) = printer(Format::Json).render(&failed(), "ignored");
        assert!(stderr.is_empty());
        assert_eq!(stdout.lines().count(), 1);
        let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        insta::assert_json_snapshot!(v, { ".pogo_version" => "[version]" });
    }

    #[test]
    fn jsonl_result_line_has_type() {
        let (stdout, _) = printer(Format::Jsonl).render(&failed(), "");
        let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(v["type"], "result");
        assert_eq!(v["error"]["code"], "DEVICE_NOT_FOUND");
    }

    #[test]
    fn human_errors_and_warnings_go_to_stderr() {
        let (stdout, stderr) = printer(Format::Human).render(&failed(), "");
        assert!(stdout.is_empty());
        insta::assert_snapshot!(stderr);
    }

    #[test]
    fn quiet_hides_warnings_not_errors() {
        let p = Printer {
            quiet: true,
            ..printer(Format::Human)
        };
        let (_, stderr) = p.render(&failed(), "");
        assert!(!stderr.contains("warning"));
        assert!(stderr.contains("error[DEVICE_NOT_FOUND]"));
    }

    #[test]
    fn color_only_when_enabled() {
        let on = Style { color: true };
        let off = Style { color: false };
        assert_eq!(on.red("x"), "\x1b[31mx\x1b[0m");
        assert_eq!(off.red("x"), "x");
    }
}
