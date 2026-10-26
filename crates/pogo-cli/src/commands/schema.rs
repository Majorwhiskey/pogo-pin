//! `pogo schema <COMMAND> | --all [--out DIR]`.

use std::path::{Path, PathBuf};

use clap::CommandFactory;
use pogo_core::schema::{COMMANDS_WITH_SCHEMA, envelope_schema};
use pogo_core::{ErrorBody, ErrorCode, PogoError};
use serde::Serialize;

use crate::cli::Cli;
use crate::output::{Render, Reply, Style};

/// A schema prints as a bare JSON document; everything else is a normal reply.
pub enum SchemaOutput {
    Raw(String),
    Reply(Reply),
}

/// `data` for `pogo schema --all --out DIR`.
#[derive(Serialize)]
pub struct SchemaWriteResult {
    pub files: Vec<PathBuf>,
}

impl Render for SchemaWriteResult {
    fn render_human(&self, out: &mut String, _style: &Style) {
        for f in &self.files {
            out.push_str(&format!("wrote {}\n", f.display()));
        }
    }
}

pub fn run(command: Option<&str>, out: Option<&Path>, style: &Style) -> SchemaOutput {
    if let Some(name) = command {
        return match one(name) {
            Ok(schema) => SchemaOutput::Raw(pretty(&schema)),
            Err(e) => SchemaOutput::Reply(Reply::error(e)),
        };
    }
    let all = COMMANDS_WITH_SCHEMA
        .iter()
        .filter_map(|c| envelope_schema(c).map(|s| (c.to_string(), s)));
    let Some(dir) = out else {
        let map: serde_json::Map<String, serde_json::Value> = all.collect();
        return SchemaOutput::Raw(pretty(&serde_json::Value::Object(map)));
    };
    let written = std::fs::create_dir_all(dir)
        .and_then(|()| {
            all.map(|(cmd, schema)| {
                let path = dir.join(format!("{cmd}.json"));
                std::fs::write(&path, pretty(&schema)).map(|()| path)
            })
            .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|e| {
            PogoError::Internal(format!("cannot write schemas to {}: {e}", dir.display()))
        });
    SchemaOutput::Reply(match written {
        Ok(files) => Reply::ok(&SchemaWriteResult { files }, style),
        Err(e) => Reply::error(&e),
    })
}

fn one(name: &str) -> Result<serde_json::Value, ErrorBody> {
    if let Some(schema) = envelope_schema(name) {
        return Ok(schema);
    }
    let known = command_names();
    if known.iter().any(|k| k == name) {
        return Err(ErrorBody::new(
            ErrorCode::NotImplemented,
            format!("'{name}' has no data schema yet"),
        )
        .with_hint(format!(
            "Commands with schemas: {}",
            COMMANDS_WITH_SCHEMA.join(", ")
        )));
    }
    Err(
        ErrorBody::new(ErrorCode::Usage, format!("unknown command '{name}'"))
            .with_hint(format!("Known commands: {}", known.join(", "))),
    )
}

/// Dotted names of every leaf command in the clap tree, e.g. `serial.capture`.
pub fn command_names() -> Vec<String> {
    fn walk(cmd: &clap::Command, prefix: &str, out: &mut Vec<String>) {
        for sub in cmd.get_subcommands() {
            let name = if prefix.is_empty() {
                sub.get_name().to_string()
            } else {
                format!("{prefix}.{}", sub.get_name())
            };
            if sub.has_subcommands() {
                walk(sub, &name, out);
            } else {
                out.push(name);
            }
        }
    }
    let mut out = Vec::new();
    walk(&Cli::command(), "", &mut out);
    out
}

fn pretty(v: &serde_json::Value) -> String {
    let mut s = serde_json::to_string_pretty(v).unwrap_or_default();
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_commands_are_real_commands() {
        let names = command_names();
        assert!(names.contains(&"serial.capture".to_string()));
        assert!(!names.contains(&"serial".to_string()));
        for c in COMMANDS_WITH_SCHEMA {
            assert!(names.contains(&c.to_string()), "{c}");
        }
    }
}
