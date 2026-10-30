//! clap definitions for every command (docs/commands.md §7).

mod args;
mod groups;
mod parse;

use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};

pub use args::*;
pub use groups::*;
use parse::parse_duration;

use crate::help;
use crate::output::Format;

#[derive(Debug, Parser)]
#[command(
    name = "pogo",
    // Fixed so help and usage text read the same on every OS (not `pogo.exe`).
    bin_name = "pogo",
    version,
    about = "Agent-friendly hardware workbench: flash, monitor, and probe ESP32 boards",
    long_about = None,
    after_long_help = help::ROOT,
    propagate_version = true,
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Args)]
#[command(next_help_heading = "Global options")]
pub struct GlobalArgs {
    /// Target device: alias, USB serial, port, or "auto"
    #[arg(short, long, global = true, value_name = "SELECTOR")]
    pub device: Option<String>,

    /// Probe device [default: "probe" alias, else auto]
    #[arg(long, global = true, value_name = "SELECTOR")]
    pub probe: Option<String>,

    /// Output format [default: human]
    #[arg(short, long, global = true, env = "POGO_FORMAT", value_enum)]
    pub format: Option<Format>,

    /// Config file
    #[arg(long, global = true, env = "POGO_CONFIG", value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Overall deadline for this command, e.g. 500ms, 10s, 2m
    #[arg(long, global = true, value_name = "DURATION", value_parser = parse_duration)]
    pub timeout: Option<Duration>,

    /// Wait for a busy device instead of failing
    #[arg(long, global = true, value_name = "DURATION", value_parser = parse_duration)]
    pub wait_lock: Option<Duration>,

    /// Describe what a mutating command would do; change nothing
    #[arg(long, global = true)]
    pub dry_run: bool,

    /// More logs on stderr (repeatable)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Errors only on stderr
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Disable colors
    #[arg(long, global = true)]
    pub no_color: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List connected serial devices and classify them
    #[command(after_long_help = help::DEVICES)]
    Devices {
        /// Also show unrecognized serial devices
        #[arg(long)]
        all: bool,
    },

    /// Check the environment: config, devices, permissions, probe
    #[command(after_long_help = help::DOCTOR)]
    Doctor {
        /// Also open ports and perform the probe handshake
        #[arg(long)]
        deep: bool,
    },

    /// Read chip, revision, features, MAC, and flash size (resets the target)
    #[command(after_long_help = help::INFO)]
    Info,

    /// Flash an ELF or raw .bin image to the target
    #[command(after_long_help = help::FLASH)]
    Flash(FlashArgs),

    /// Reset the target into run mode or the bootloader
    #[command(after_long_help = help::RESET)]
    Reset {
        /// Mode to boot into
        #[arg(long, value_enum, default_value_t = ResetMode::Run)]
        mode: ResetMode,
        /// How to toggle reset
        #[arg(long, value_enum, default_value_t = ResetVia::Auto)]
        via: ResetVia,
    },

    /// Capture and send serial data
    #[command(subcommand)]
    Serial(SerialCommand),

    /// Flash, reset, and capture in one step (the main agent command)
    #[command(after_long_help = help::RUN)]
    Run {
        #[command(flatten)]
        flash: FlashArgs,
        #[command(flatten)]
        capture: CaptureArgs,
        /// Skip flashing; reset and capture only
        #[arg(long)]
        no_flash: bool,
    },

    /// I2C bus access through the probe
    #[command(subcommand)]
    I2c(I2cCommand),

    /// GPIO control through the probe
    #[command(subcommand)]
    Gpio(GpioCommand),

    /// Current measurement and power switching through the probe
    #[command(subcommand)]
    Power(PowerCommand),

    /// Probe board information and self-test
    #[command(subcommand)]
    Probe(ProbeCommand),

    /// Inspect and validate configuration
    #[command(subcommand)]
    Config(ConfigCommand),

    /// Print the JSON Schema for a command's envelope
    #[command(after_long_help = help::SCHEMA)]
    Schema {
        /// Dotted command name, e.g. serial.capture
        #[arg(required_unless_present = "all", conflicts_with = "all")]
        command: Option<String>,
        /// Generate schemas for every command
        #[arg(long)]
        all: bool,
        /// Write schema files into this directory instead of stdout
        #[arg(long, value_name = "DIR", requires = "all")]
        out: Option<PathBuf>,
    },

    /// Print shell completions
    #[command(after_long_help = help::COMPLETIONS)]
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Start the MCP server on stdio
    #[command(after_long_help = help::MCP)]
    Mcp,
}

impl Command {
    /// Dotted command name used in the envelope, e.g. `serial.capture`.
    pub fn name(&self) -> &'static str {
        match self {
            Command::Devices { .. } => "devices",
            Command::Doctor { .. } => "doctor",
            Command::Info => "info",
            Command::Flash(_) => "flash",
            Command::Reset { .. } => "reset",
            Command::Serial(SerialCommand::Capture { .. }) => "serial.capture",
            Command::Serial(SerialCommand::Send { .. }) => "serial.send",
            Command::Run { .. } => "run",
            Command::I2c(I2cCommand::Scan { .. }) => "i2c.scan",
            Command::I2c(I2cCommand::Read { .. }) => "i2c.read",
            Command::I2c(I2cCommand::Write { .. }) => "i2c.write",
            Command::Gpio(GpioCommand::Get { .. }) => "gpio.get",
            Command::Gpio(GpioCommand::Set { .. }) => "gpio.set",
            Command::Gpio(GpioCommand::Pulse { .. }) => "gpio.pulse",
            Command::Power(PowerCommand::Measure { .. }) => "power.measure",
            Command::Power(PowerCommand::On) => "power.on",
            Command::Power(PowerCommand::Off) => "power.off",
            Command::Power(PowerCommand::Cycle { .. }) => "power.cycle",
            Command::Probe(ProbeCommand::Info) => "probe.info",
            Command::Probe(ProbeCommand::Selftest) => "probe.selftest",
            Command::Config(ConfigCommand::Show) => "config.show",
            Command::Config(ConfigCommand::Path) => "config.path",
            Command::Config(ConfigCommand::Validate) => "config.validate",
            Command::Schema { .. } => "schema",
            Command::Completions { .. } => "completions",
            Command::Mcp => "mcp",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn erase_all_requires_confirm() {
        let r = Cli::try_parse_from(["pogo", "flash", "app.elf", "--erase-all"]);
        assert!(r.is_err());
        let r = Cli::try_parse_from(["pogo", "flash", "app.elf", "--erase-all", "--confirm", "X"]);
        assert!(r.is_ok());
    }

    #[test]
    fn help_names_pogo_whatever_the_executable_is_called() {
        let err = Cli::try_parse_from(["/opt/bin/pogo-renamed", "serial", "--help"]).unwrap_err();
        assert!(err.to_string().contains("Usage: pogo serial"), "{err}");
    }
}
