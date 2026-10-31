//! Tool arguments. Field docs become the JSON Schema descriptions agents read.

use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListDevicesParams {
    /// Also list serial ports that are not recognized ESP32 boards or pogo probes.
    #[serde(default)]
    pub all: bool,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct DoctorParams {
    /// Also open each board's serial port (without resetting it).
    #[serde(default)]
    pub deep: bool,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct DeviceParams {
    /// Target device: config alias, USB serial number, port path, or "auto" (default:
    /// the "target" alias if configured, else auto; auto fails if several boards match).
    pub device: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ResetModeParam {
    /// Start the application.
    #[default]
    Run,
    /// Hold the boot strap so the chip waits in the ROM download mode.
    Bootloader,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ResetParams {
    /// Target device (see list_devices).
    pub device: Option<String>,
    /// "run" (default) or "bootloader".
    #[serde(default)]
    pub mode: ResetModeParam,
    /// Report what would happen without touching the device.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct FlashParams {
    /// Path to an ELF (preferred) or raw .bin image on this machine.
    pub image: String,
    /// Flash address for a raw .bin, hex like "0x10000". Required for .bin, ignored for ELF.
    pub address: Option<String>,
    /// Chip type such as "esp32" or "esp32c3" (default: auto-detect).
    pub chip: Option<String>,
    /// Baud rate while flashing (default: 115200).
    pub flash_baud: Option<u32>,
    /// Skip read-back verification (default: verify).
    #[serde(default)]
    pub no_verify: bool,
    /// Stay in the bootloader afterwards instead of starting the app (default: start it).
    #[serde(default)]
    pub no_reset: bool,
    /// Allow raw writes below 0x10000 (bootloader and partition table). Refused otherwise.
    #[serde(default)]
    pub allow_bootloader_write: bool,
    /// Check guardrails and report the plan without touching the device.
    #[serde(default)]
    pub dry_run: bool,
    /// Target device (see list_devices).
    pub device: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ParserParam {
    /// Try ESP-IDF, then Rust log format, per line.
    #[default]
    Auto,
    EspIdf,
    RustLog,
    Raw,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct CaptureParams {
    /// Stop with success when a line matches any of these regexes.
    #[serde(default)]
    pub until: Vec<String>,
    /// Extra regexes that end the capture as a target fault (added to the built-in panic,
    /// abort, brownout, and watchdog patterns).
    #[serde(default)]
    pub fail_on: Vec<String>,
    /// Drop the built-in fail-on patterns.
    #[serde(default)]
    pub no_default_fail_patterns: bool,
    /// Keep reading this long after an `until` match to catch late crashes, e.g. "2s"
    /// (default: 0).
    pub settle: Option<String>,
    /// Maximum lines returned (default 500); the excess is dropped from the middle.
    pub max_lines: Option<usize>,
    /// Log line parser (default: the device's configured parser, else "auto").
    pub parser: Option<ParserParam>,
    /// ELF used to turn panic backtrace addresses into function, file, and line.
    pub elf: Option<String>,
    /// Baud rate (default: config, else 115200).
    pub baud: Option<u32>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SerialCaptureParams {
    #[serde(flatten)]
    pub capture: CaptureParams,
    /// Reset the target first and capture from the first boot byte; a second boot is
    /// then reported as an unexpected reboot. Without it the target is not touched.
    #[serde(default)]
    pub reset: bool,
    /// Overall deadline, e.g. "10s" (default: config defaults.timeout, else 10s).
    pub timeout: Option<String>,
    /// Target device (see list_devices).
    pub device: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LineEndingParam {
    #[default]
    Lf,
    Crlf,
    None,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SerialSendParams {
    /// Text to send, or hex bytes like "de ad be ef" when `hex` is true.
    pub data: String,
    /// Treat `data` as hex bytes.
    #[serde(default)]
    pub hex: bool,
    /// Line ending appended to the data (default "lf").
    #[serde(default)]
    pub line_ending: LineEndingParam,
    /// Capture until a reply line matches this regex (with fail-on detection). Without
    /// it nothing is read back.
    pub expect: Option<String>,
    /// Deadline for the reply, e.g. "5s" (default: config defaults.timeout, else 5s).
    pub timeout: Option<String>,
    /// Target device (see list_devices).
    pub device: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct RunParams {
    /// Path to the ELF (or .bin with `address`) to flash; also used to symbolize panics.
    pub image: String,
    /// Flash address for a raw .bin, hex like "0x10000".
    pub address: Option<String>,
    /// Skip flashing; just reset and capture.
    #[serde(default)]
    pub no_flash: bool,
    #[serde(flatten)]
    pub capture: CaptureParams,
    /// Report the flash plan without touching the device.
    #[serde(default)]
    pub dry_run: bool,
    /// Capture deadline after flashing, e.g. "10s" (default: config defaults.timeout, else 10s).
    pub timeout: Option<String>,
    /// Target device (see list_devices).
    pub device: Option<String>,
}
