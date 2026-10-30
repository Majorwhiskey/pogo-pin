mod cli;
mod commands;
mod help;
mod interrupt;
mod output;

use std::ffi::OsString;
use std::io::IsTerminal;
use std::process::ExitCode;
use std::time::Instant;

use clap::{CommandFactory, Parser};
use pogo_core::{ErrorBody, ErrorCode, PogoError};

use crate::cli::{Cli, Command, SerialCommand};
use crate::output::{Format, Printer, Reply, Style};

fn main() -> ExitCode {
    let start = Instant::now();
    let args: Vec<OsString> = std::env::args_os().collect();

    let cli = match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(err) => return usage_error(err, &args, start),
    };

    let format = cli.global.format.unwrap_or(Format::Human);
    let style = Style::detect(format, cli.global.no_color);
    init_logging(cli.global.verbose, cli.global.quiet, style);

    let printer = Printer {
        format,
        style,
        quiet: cli.global.quiet,
    };
    let command = cli.command.name();
    let reply = dispatch(cli, format, &style);
    let code = printer.finish(command, elapsed_ms(start), reply);
    ExitCode::from(code)
}

fn dispatch(cli: Cli, format: Format, style: &Style) -> Reply {
    match cli.command {
        Command::Serial(SerialCommand::Capture { reset, capture }) => {
            commands::serial::capture(&cli.global, reset, &capture, format, style)
        }
        Command::Serial(SerialCommand::Send {
            data,
            line_ending,
            hex,
            expect,
        }) => commands::serial_send::send(
            &cli.global,
            commands::serial_send::SendArgs {
                data: &data,
                line_ending,
                hex,
                expect: expect.as_deref(),
            },
            format,
            style,
        ),
        Command::Devices { all } => commands::devices::run(all, cli.global.config, style),
        Command::Config(cmd) => commands::config::run(&cmd, cli.global.config, style),
        Command::Schema { command, out, .. } => {
            match commands::schema::run(command.as_deref(), out.as_deref(), style) {
                commands::schema::SchemaOutput::Reply(reply) => reply,
                commands::schema::SchemaOutput::Raw(text) => {
                    use std::io::Write;
                    let _ = std::io::stdout().lock().write_all(text.as_bytes());
                    // The schema document is the output; there is no envelope.
                    std::process::exit(0);
                }
            }
        }
        Command::Completions { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "pogo", &mut std::io::stdout());
            // The script is the output; there is no envelope.
            std::process::exit(0);
        }
        other => Reply::error(&PogoError::NotImplemented(other.name())),
    }
}

/// clap failed to parse. Help and version print normally; real usage errors become a
/// `USAGE` envelope in json/jsonl mode so agents always get parseable output.
fn usage_error(err: clap::Error, args: &[OsString], start: Instant) -> ExitCode {
    use clap::error::ErrorKind;
    if matches!(
        err.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    ) && err.exit_code() == 0
    {
        err.exit();
    }

    let format = format_from_raw_args(args);
    if !matches!(format, Some(Format::Json | Format::Jsonl)) {
        err.exit();
    }

    // clap's first paragraph is the error itself; the rest is usage and a help pointer.
    let rendered = err.to_string();
    let message = rendered
        .lines()
        .take_while(|l| !l.trim().is_empty())
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ");
    let message = message.trim_start_matches("error: ");
    let reply = Reply::error(
        ErrorBody::new(ErrorCode::Usage, message)
            .with_hint("Run `pogo --help` or `pogo <command> --help` for usage and examples."),
    );
    let printer = Printer {
        format: format.unwrap_or(Format::Json),
        style: Style { color: false },
        quiet: true,
    };
    ExitCode::from(printer.finish("pogo", elapsed_ms(start), reply))
}

/// Best-effort `--format` lookup when clap could not parse the arguments.
fn format_from_raw_args(args: &[OsString]) -> Option<Format> {
    use clap::ValueEnum;
    let parse = |s: &str| Format::from_str(s, true).ok();
    let mut iter = args.iter().skip(1).filter_map(|a| a.to_str());
    while let Some(arg) = iter.next() {
        if arg == "--" {
            break;
        }
        if let Some(v) = arg.strip_prefix("--format=") {
            return parse(v);
        }
        if arg == "--format" || arg == "-f" {
            return iter.next().and_then(parse);
        }
        if let Some(v) = arg.strip_prefix("-f").filter(|v| !v.is_empty()) {
            return parse(v.trim_start_matches('='));
        }
    }
    std::env::var("POGO_FORMAT").ok().as_deref().and_then(parse)
}

fn init_logging(verbose: u8, quiet: bool, style: Style) {
    use tracing::Level;
    let level = match (quiet, verbose) {
        (true, _) => Level::ERROR,
        (false, 0) => Level::WARN,
        (false, 1) => Level::INFO,
        (false, 2) => Level::DEBUG,
        (false, _) => Level::TRACE,
    };
    let _ = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .with_ansi(style.color && std::io::stderr().is_terminal())
        .with_target(false)
        .try_init();
}

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(args: &[&str]) -> Vec<OsString> {
        std::iter::once("pogo")
            .chain(args.iter().copied())
            .map(OsString::from)
            .collect()
    }

    #[test]
    fn raw_format_detection() {
        assert_eq!(
            format_from_raw_args(&raw(&["-f", "json", "x"])),
            Some(Format::Json)
        );
        assert_eq!(
            format_from_raw_args(&raw(&["--format=jsonl"])),
            Some(Format::Jsonl)
        );
        assert_eq!(format_from_raw_args(&raw(&["-fjson"])), Some(Format::Json));
        assert_eq!(
            format_from_raw_args(&raw(&["--format", "human"])),
            Some(Format::Human)
        );
    }
}
