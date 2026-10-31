//! Arguments for the tools that go through the pogo probe.

use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ProbeTarget {
    /// The pogo probe: config alias, USB serial number, port path, or "auto" (default:
    /// the "probe" alias if configured, else auto).
    pub probe: Option<String>,
    /// If another command is using the probe, wait up to this long for it, e.g. "30s"
    /// (default: fail at once with DEVICE_BUSY).
    pub wait_lock: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct I2cScanParams {
    /// 0 (user bus, default) or 1 (the probe's INA226 bus).
    #[serde(default)]
    pub bus: u8,
    /// First address to probe (default 8).
    pub first: Option<u8>,
    /// Last address to probe (default 119, i.e. 0x77).
    pub last: Option<u8>,
    #[serde(flatten)]
    pub target: ProbeTarget,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct I2cReadParams {
    /// 7-bit device address, decimal (e.g. 104 for 0x68).
    pub addr: u8,
    /// Register to write before reading (repeated start), if any.
    pub reg: Option<u8>,
    /// Bytes to read, 1 to 64.
    pub len: u8,
    /// 0 (user bus, default) or 1.
    #[serde(default)]
    pub bus: u8,
    #[serde(flatten)]
    pub target: ProbeTarget,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct I2cWriteParams {
    /// 7-bit device address, decimal.
    pub addr: u8,
    /// Bytes to write, 1 to 64, e.g. [107, 0].
    pub bytes: Vec<u8>,
    /// Only bus 0; bus 1 belongs to the INA226 and is refused.
    #[serde(default)]
    pub bus: u8,
    /// Check guardrails and report the plan without touching the bus.
    #[serde(default)]
    pub dry_run: bool,
    #[serde(flatten)]
    pub target: ProbeTarget,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PinStateParam {
    Low,
    High,
    /// Stop driving the pin (high impedance).
    #[default]
    Release,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct GpioParams {
    /// Pin name from the config's [pins] table, e.g. "target_en".
    pub pin: String,
    #[serde(flatten)]
    pub target: ProbeTarget,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct GpioSetParams {
    /// Pin name from the config's [pins] table.
    pub pin: String,
    /// "low", "high", or "release". Open-drain pins refuse "high".
    pub state: PinStateParam,
    /// Check guardrails and report the plan without touching the pin.
    #[serde(default)]
    pub dry_run: bool,
    #[serde(flatten)]
    pub target: ProbeTarget,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct GpioPulseParams {
    /// Pin name from the config's [pins] table.
    pub pin: String,
    /// Level held during the pulse: "low" (default) or "high".
    pub level: Option<PinStateParam>,
    /// Pulse length, e.g. "100ms" (default), up to 60s.
    pub duration: Option<String>,
    /// Check guardrails and report the plan without touching the pin.
    #[serde(default)]
    pub dry_run: bool,
    #[serde(flatten)]
    pub target: ProbeTarget,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct PowerMeasureParams {
    /// How long to sample, e.g. "5s" (default).
    pub duration: Option<String>,
    /// Time between samples, e.g. "1ms" (default), 100µs to 1s.
    pub interval: Option<String>,
    /// Also write raw samples as CSV to this path on this machine.
    pub out: Option<String>,
    #[serde(flatten)]
    pub target: ProbeTarget,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PowerAction {
    On,
    #[default]
    Off,
    /// Off, wait `off_time`, then on.
    Cycle,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct PowerSwitchParams {
    /// "on", "off", or "cycle".
    pub action: PowerAction,
    /// For cycle: how long power stays off, e.g. "500ms" (default).
    pub off_time: Option<String>,
    /// Check guardrails and report the plan without switching.
    #[serde(default)]
    pub dry_run: bool,
    #[serde(flatten)]
    pub target: ProbeTarget,
}
