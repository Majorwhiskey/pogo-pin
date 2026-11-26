//! The launcher menu for bare `pogo` (or `pogo-pin`) in an interactive terminal. Each
//! choice runs this same binary with ordinary arguments, so the menu adds nothing an
//! agent could not do with the commands, and agents never see it.

mod screen;

use std::ffi::OsString;
use std::io::{BufRead, IsTerminal, Write};
use std::process::{Command, ExitCode};
use std::sync::atomic::{AtomicBool, Ordering};

use pogo_core::discovery::DeviceKind;
use pogo_core::ops::devices::devices;

use crate::output::{Format, Style};
use screen::{Action, Status};

static CHILD_RUNNING: AtomicBool = AtomicBool::new(false);

/// Only a person at a terminal gets the menu. Pipes, agents, and `POGO_FORMAT` users
/// keep the usual usage error for a missing command.
pub fn wanted(args: &[OsString]) -> bool {
    args.len() == 1
        && std::env::var_os("POGO_FORMAT").is_none()
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
}

pub fn run() -> ExitCode {
    // Ctrl-C belongs to the command running under the menu; at the prompt it quits.
    let _ = ctrlc::set_handler(|| {
        if !CHILD_RUNNING.load(Ordering::SeqCst) {
            std::process::exit(130);
        }
    });
    let style = Style::detect(Format::Human, false);
    let mut input = std::io::stdin().lock();
    loop {
        print!(
            "{}",
            screen::render(&style, env!("CARGO_PKG_VERSION"), &status())
        );
        let Some(choice) = ask(&mut input, &screen::prompt(&style)) else {
            return ExitCode::SUCCESS;
        };
        let Some(item) = screen::pick(&choice) else {
            println!("{}", style.yellow(&format!("'{choice}' is not an option.")));
            continue;
        };
        let args: Vec<String> = match item.action {
            Action::Exit => return ExitCode::SUCCESS,
            Action::Quickstart => {
                println!("\n{}", screen::QUICKSTART);
                Vec::new()
            }
            Action::Run(args) => args.iter().map(|a| a.to_string()).collect(),
            Action::FlashRun => {
                let Some(elf) = ask(&mut input, "ELF to flash: ").filter(|e| !e.is_empty()) else {
                    continue;
                };
                let until = ask(&mut input, "Wait for a line matching (READY): ")
                    .filter(|u| !u.is_empty())
                    .unwrap_or_else(|| "READY".into());
                if !confirm(&mut input, &style, "This flashes the connected board.") {
                    continue;
                }
                vec!["run".into(), elf, "--until".into(), until]
            }
            Action::Reset => {
                if !confirm(&mut input, &style, "This resets the connected board.") {
                    continue;
                }
                vec!["reset".into()]
            }
            Action::Tests => {
                let file = ask(&mut input, "Suite file (pogo-test.toml): ")
                    .filter(|f| !f.is_empty())
                    .unwrap_or_else(|| "pogo-test.toml".into());
                vec!["test".into(), file]
            }
        };
        if !args.is_empty() {
            run_pogo(&style, &args);
        }
        if ask(&mut input, &style.dim("Press Enter to return to the menu")).is_none() {
            return ExitCode::SUCCESS;
        }
    }
}

/// Print `prompt` and read one line; `None` at end of input.
fn ask(input: &mut impl BufRead, prompt: &str) -> Option<String> {
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    match input.read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line.trim().to_string()),
    }
}

fn confirm(input: &mut impl BufRead, style: &Style, what: &str) -> bool {
    let answer = ask(input, &format!("{} Continue? [y/N] ", style.yellow(what)));
    answer.is_some_and(|a| a.eq_ignore_ascii_case("y") || a.eq_ignore_ascii_case("yes"))
}

fn run_pogo(style: &Style, args: &[String]) {
    println!("{}", style.dim(&format!("$ pogo {}", args.join(" "))));
    let status = std::env::current_exe().and_then(|exe| {
        CHILD_RUNNING.store(true, Ordering::SeqCst);
        let status = Command::new(exe).args(args).status();
        CHILD_RUNNING.store(false, Ordering::SeqCst);
        status
    });
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => println!(
            "{}",
            style.yellow(&format!("exited with code {}", s.code().unwrap_or(1)))
        ),
        Err(e) => println!("{}", style.red(&format!("cannot start pogo: {e}"))),
    }
}

/// Boards and probe on the USB ports right now. Listing ports never opens them.
fn status() -> Status {
    let found = devices(None, false)
        .data
        .map(|d| d.devices)
        .unwrap_or_default();
    let ports = |kind: DeviceKind| {
        found
            .iter()
            .filter(move |d| d.kind == kind)
            .map(|d| d.port.clone())
    };
    Status {
        boards: ports(DeviceKind::Esp32Target).collect(),
        probe: ports(DeviceKind::PogoProbe).next(),
    }
}
