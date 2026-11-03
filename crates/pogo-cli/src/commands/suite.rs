//! `pogo test`: run a suite file and render its results.

use std::path::PathBuf;

use pogo_core::ops::suite::{SuiteRequest, run_suite};
use pogo_core::suite::{Status, SuiteData};

use crate::cli::GlobalArgs;
use crate::commands::target::{reply, selection};
use crate::interrupt::interrupt_flag;
use crate::output::{Render, Reply, Style};

pub fn run(
    global: &GlobalArgs,
    file: PathBuf,
    junit: Option<PathBuf>,
    no_flash: bool,
    fail_fast: bool,
    style: &Style,
) -> Reply {
    let req = SuiteRequest {
        file,
        no_flash,
        fail_fast,
        junit,
    };
    reply(
        run_suite(&selection(global), &req, Some(interrupt_flag())),
        style,
    )
}

impl Render for SuiteData {
    fn render_human(&self, out: &mut String, style: &Style) {
        if let Some(f) = &self.flash {
            out.push_str(&style.dim(&format!("flashed {} in {} ms\n", f.chip, f.duration_ms)));
        }
        for t in &self.tests {
            let mark = match t.status {
                Status::Passed => style.green("pass"),
                Status::Failed => style.red("FAIL"),
            };
            out.push_str(&format!("{mark}  {}  ({} ms)\n", t.name, t.duration_ms));
            if let Some(f) = &t.failure {
                out.push_str(&format!("      step {}: {}\n", f.step, f.message));
                for line in &f.lines {
                    out.push_str(&style.dim(&format!("      | {line}\n")));
                }
            }
        }
        out.push_str(&format!(
            "{} passed, {} failed ({})\n",
            self.passed, self.failed, self.device.port
        ));
        if let Some(p) = &self.junit_path {
            out.push_str(&style.dim(&format!("JUnit report: {p}\n")));
        }
    }
}
