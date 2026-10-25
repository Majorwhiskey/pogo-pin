//! clap definitions for every command (README §7).

use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::help;
use crate::output::Format;

#[derive(Debug, Parser)]
#[command(
    name = "pogo",
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

#[derive(Debug, Args)]
pub struct FlashArgs {
    /// ELF (preferred) or raw .bin image
    pub image: PathBuf,
    /// Flash address for a raw .bin, e.g. 0x10000
    #[arg(long, value_name = "ADDR", value_parser = parse_hex_u32)]
    pub address: Option<u32>,
    /// Chip type [default: auto-detect]
    #[arg(long, value_name = "CHIP")]
    pub chip: Option<String>,
    /// Baud rate used while flashing
    #[arg(long, value_name = "BAUD")]
    pub flash_baud: Option<u32>,
    /// Custom bootloader image
    #[arg(long, value_name = "BIN")]
    pub bootloader: Option<PathBuf>,
    /// Custom partition table
    #[arg(long, value_name = "CSV|BIN")]
    pub partition_table: Option<PathBuf>,
    /// Skip read-back verification
    #[arg(long)]
    pub no_verify: bool,
    /// Stay in the bootloader after flashing
    #[arg(long)]
    pub no_reset: bool,
    /// Erase the entire flash first (guarded; requires --confirm)
    #[arg(long, requires = "confirm")]
    pub erase_all: bool,
    /// USB serial of the device, confirming --erase-all
    #[arg(long, value_name = "USB_SERIAL", requires = "erase_all")]
    pub confirm: Option<String>,
    /// Allow raw writes to the bootloader and partition table area
    #[arg(long)]
    pub allow_bootloader_write: bool,
}

#[derive(Debug, Args)]
pub struct CaptureArgs {
    /// Baud rate
    #[arg(long, value_name = "N")]
    pub baud: Option<u32>,
    /// Stop with success when a line matches (repeatable)
    #[arg(long, value_name = "REGEX")]
    pub until: Vec<String>,
    /// Stop with TARGET_FAULT when a line matches (repeatable)
    #[arg(long, value_name = "REGEX")]
    pub fail_on: Vec<String>,
    /// Don't use the built-in panic/abort/brownout patterns
    #[arg(long)]
    pub no_default_fail_patterns: bool,
    /// Keep reading this long after an --until match to catch late crashes
    #[arg(long, value_name = "DURATION", value_parser = parse_duration)]
    pub settle: Option<Duration>,
    /// Maximum lines returned; extra lines are omitted from the middle
    #[arg(long, value_name = "N", default_value_t = 500)]
    pub max_lines: usize,
    /// Maximum characters per line; longer lines are truncated
    #[arg(long, value_name = "N", default_value_t = 1000)]
    pub max_line_len: usize,
    /// Log line parser
    #[arg(long, value_enum, default_value_t = LogParser::Auto)]
    pub parser: LogParser,
    /// ELF used to symbolize backtraces
    #[arg(long, value_name = "PATH")]
    pub elf: Option<PathBuf>,
    /// Keep ANSI escape codes
    #[arg(long)]
    pub raw: bool,
    /// Save the raw byte stream with timestamps as JSONL
    #[arg(long, value_name = "FILE")]
    pub record: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum SerialCommand {
    /// Capture serial output until a pattern, fault, or deadline
    #[command(after_long_help = help::SERIAL_CAPTURE)]
    Capture {
        /// Reset into run mode first and capture from the first boot byte
        #[arg(long)]
        reset: bool,
        #[command(flatten)]
        capture: CaptureArgs,
    },
    /// Write data to the target, optionally waiting for a response
    #[command(after_long_help = help::SERIAL_SEND)]
    Send {
        /// Data to send, or "-" to read stdin
        data: String,
        /// Line ending appended to the data
        #[arg(long, value_enum, default_value_t = LineEnding::Lf)]
        line_ending: LineEnding,
        /// Interpret data as hex bytes, e.g. "de ad be ef"
        #[arg(long)]
        hex: bool,
        /// Capture until a response line matches
        #[arg(long, value_name = "REGEX")]
        expect: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum I2cCommand {
    /// Scan a bus for responding addresses
    #[command(after_long_help = help::I2C_SCAN)]
    Scan {
        #[arg(long, default_value_t = 0)]
        bus: u8,
        /// Address range, e.g. 0x08-0x77
        #[arg(long, value_parser = parse_i2c_range, default_value = "0x08-0x77")]
        range: (u8, u8),
    },
    /// Read bytes from a device, optionally from a register
    #[command(after_long_help = help::I2C_READ)]
    Read {
        #[arg(value_parser = parse_hex_u8)]
        addr: u8,
        /// Register to read from (write then repeated-start read)
        #[arg(long, value_parser = parse_hex_u8)]
        reg: Option<u8>,
        /// Number of bytes to read
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=64))]
        len: u8,
        #[arg(long, default_value_t = 0)]
        bus: u8,
    },
    /// Write bytes to a device
    #[command(after_long_help = help::I2C_WRITE)]
    Write {
        #[arg(value_parser = parse_hex_u8)]
        addr: u8,
        /// Bytes to write, e.g. 0x75 0x00
        #[arg(required = true, num_args = 1..=64, value_parser = parse_hex_u8)]
        bytes: Vec<u8>,
        #[arg(long, default_value_t = 0)]
        bus: u8,
    },
}

#[derive(Debug, Subcommand)]
pub enum GpioCommand {
    /// Read a pin's level
    #[command(after_long_help = help::GPIO_GET)]
    Get {
        /// Pin name from the config's [pins] table
        pin: String,
    },
    /// Drive or release a pin
    #[command(after_long_help = help::GPIO_SET)]
    Set {
        /// Pin name from the config's [pins] table
        pin: String,
        #[arg(value_enum)]
        level: PinState,
    },
    /// Pulse a pin for a precise duration (timed on the probe)
    #[command(after_long_help = help::GPIO_PULSE)]
    Pulse {
        /// Pin name from the config's [pins] table
        pin: String,
        #[arg(long, value_enum, default_value_t = PulseLevel::Low)]
        level: PulseLevel,
        #[arg(long, value_parser = parse_duration, default_value = "100ms")]
        duration: Duration,
    },
}

#[derive(Debug, Subcommand)]
pub enum PowerCommand {
    /// Measure current and voltage with the INA226
    #[command(after_long_help = help::POWER_MEASURE)]
    Measure {
        #[arg(long, value_parser = parse_duration, default_value = "5s")]
        duration: Duration,
        #[arg(long, value_parser = parse_duration, default_value = "1ms")]
        interval: Duration,
        /// Also write raw samples to a CSV file
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },
    /// Switch target power on
    #[command(after_long_help = help::POWER_SWITCH)]
    On,
    /// Switch target power off
    #[command(after_long_help = help::POWER_SWITCH)]
    Off,
    /// Switch target power off, wait, then on
    #[command(after_long_help = help::POWER_CYCLE)]
    Cycle {
        #[arg(long, value_parser = parse_duration, default_value = "500ms")]
        off_time: Duration,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProbeCommand {
    /// Firmware version, protocol version, board, uptime, driven pins
    #[command(after_long_help = help::PROBE_INFO)]
    Info,
    /// Check INA226 presence, pin release, and idle I2C buses
    #[command(after_long_help = help::PROBE_SELFTEST)]
    Selftest,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print the merged config and where each value came from
    #[command(after_long_help = help::CONFIG_SHOW)]
    Show,
    /// Print the config files that were found, in merge order
    #[command(after_long_help = help::CONFIG_PATH)]
    Path,
    /// Check the config for errors
    #[command(after_long_help = help::CONFIG_VALIDATE)]
    Validate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ResetMode {
    Run,
    Bootloader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ResetVia {
    Auto,
    DtrRts,
    Probe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LogParser {
    Auto,
    EspIdf,
    RustLog,
    Raw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LineEnding {
    Lf,
    Crlf,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PinState {
    Low,
    High,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PulseLevel {
    Low,
    High,
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

fn parse_duration(s: &str) -> Result<Duration, String> {
    humantime::parse_duration(s).map_err(|e| format!("{e} (examples: 500ms, 10s, 2m)"))
}

fn parse_hex_u32(s: &str) -> Result<u32, String> {
    let digits = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .ok_or_else(|| format!("expected a hex value like 0x10000, got '{s}'"))?;
    u32::from_str_radix(digits, 16).map_err(|e| format!("invalid hex value '{s}': {e}"))
}

/// Accepts `0x76` or decimal `118`.
fn parse_hex_u8(s: &str) -> Result<u8, String> {
    let parsed = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(digits) => u8::from_str_radix(digits, 16),
        None => s.parse(),
    };
    parsed.map_err(|_| format!("expected a byte like 0x76 or 118, got '{s}'"))
}

fn parse_i2c_range(s: &str) -> Result<(u8, u8), String> {
    let (first, last) = s
        .split_once('-')
        .ok_or_else(|| format!("expected a range like 0x08-0x77, got '{s}'"))?;
    let (first, last) = (parse_hex_u8(first)?, parse_hex_u8(last)?);
    if first > last || last > 0x7f {
        return Err(format!("invalid 7-bit address range '{s}'"));
    }
    Ok((first, last))
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
    fn parsers() {
        assert_eq!(parse_hex_u32("0x10000"), Ok(0x10000));
        assert!(parse_hex_u32("65536").is_err());
        assert_eq!(parse_hex_u8("0x76"), Ok(0x76));
        assert_eq!(parse_hex_u8("118"), Ok(118));
        assert!(parse_hex_u8("0x100").is_err());
        assert_eq!(parse_i2c_range("0x08-0x77"), Ok((0x08, 0x77)));
        assert!(parse_i2c_range("0x77-0x08").is_err());
        assert!(parse_i2c_range("0x08-0x80").is_err());
        assert_eq!(parse_duration("500ms"), Ok(Duration::from_millis(500)));
    }

    #[test]
    fn erase_all_requires_confirm() {
        let r = Cli::try_parse_from(["pogo", "flash", "app.elf", "--erase-all"]);
        assert!(r.is_err());
        let r = Cli::try_parse_from(["pogo", "flash", "app.elf", "--erase-all", "--confirm", "X"]);
        assert!(r.is_ok());
    }
}
