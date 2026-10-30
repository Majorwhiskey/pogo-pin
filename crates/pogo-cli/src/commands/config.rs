//! `pogo config show|path|validate`.

use std::path::PathBuf;

use pogo_core::config::{
    self, ConfigFileKind, ConfigPathResult, ConfigShowResult, ConfigValidateResult,
};
use pogo_core::ops::target::config_search;

use crate::cli::ConfigCommand;
use crate::output::{Render, Reply, Style};

pub fn run(cmd: &ConfigCommand, explicit: Option<PathBuf>, style: &Style) -> Reply {
    let search = match config_search(explicit) {
        Ok(s) => s,
        Err(e) => return Reply::error(&e),
    };
    match cmd {
        ConfigCommand::Path => Reply::ok(
            &ConfigPathResult {
                files: config::discover(&search),
            },
            style,
        ),
        ConfigCommand::Show => match config::load(&search) {
            Ok(loaded) => Reply::ok(&loaded.show(), style),
            Err(e) => Reply::error(&e),
        },
        ConfigCommand::Validate => match config::load(&search) {
            Ok(loaded) => Reply::ok(&loaded.validated(), style),
            Err(e) => Reply::error(&e),
        },
    }
}

const NO_FILES: &str = "No config files found; built-in defaults apply.\n";

impl Render for ConfigPathResult {
    fn render_human(&self, out: &mut String, style: &Style) {
        for f in &self.files {
            let kind = match f.kind {
                ConfigFileKind::User => "user",
                ConfigFileKind::Project => "project",
                ConfigFileKind::Explicit => "explicit",
            };
            let missing = if f.exists { "" } else { "  (not found)" };
            out.push_str(&format!(
                "{kind:<9}{}{}\n",
                f.path.display(),
                style.dim(missing)
            ));
        }
    }
}

impl Render for ConfigShowResult {
    fn render_human(&self, out: &mut String, style: &Style) {
        if self.sources.is_empty() {
            out.push_str(NO_FILES);
            return;
        }
        for s in &self.sources {
            let pointer = format!("/{}", s.key.replace('.', "/"));
            let value = self
                .config
                .pointer(&pointer)
                .map(|v| v.to_string())
                .unwrap_or_default();
            let comment = format!("# {}", s.source.display());
            out.push_str(&format!("{} = {value}  {}\n", s.key, style.dim(&comment)));
        }
    }
}

impl Render for ConfigValidateResult {
    fn render_human(&self, out: &mut String, _style: &Style) {
        if self.files.is_empty() {
            out.push_str(NO_FILES);
            return;
        }
        out.push_str("Config is valid:\n");
        for f in &self.files {
            out.push_str(&format!("  {}\n", f.display()));
        }
    }
}
