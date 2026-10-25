//! Subcommand groups: serial, i2c, gpio, power, probe, config.

use std::path::PathBuf;
use std::time::Duration;

use clap::Subcommand;

use super::args::{CaptureArgs, LineEnding, PinState, PulseLevel};
use super::parse::{parse_duration, parse_hex_u8, parse_i2c_range};
use crate::help;

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
