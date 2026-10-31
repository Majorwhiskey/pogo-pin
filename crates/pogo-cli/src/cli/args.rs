//! Shared argument groups and value enums.

use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, ValueEnum};

use super::parse::{parse_duration, parse_hex_u32};

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
    /// Log line parser [default: the device's config, else auto]
    #[arg(long, value_enum)]
    pub parser: Option<LogParser>,
    /// ELF used to symbolize backtraces
    #[arg(long, value_name = "PATH")]
    pub elf: Option<PathBuf>,
    /// Keep ANSI escape codes
    #[arg(long)]
    pub raw: bool,
    /// How long to wait for a vanished device to reappear (USB re-enumeration); 0 disables
    #[arg(long, value_name = "DURATION", value_parser = parse_duration, default_value = "5s")]
    pub reconnect_timeout: Duration,
    /// Save the raw byte stream with timestamps as JSONL
    #[arg(long, value_name = "FILE")]
    pub record: Option<PathBuf>,
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
