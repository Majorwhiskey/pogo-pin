//! USB enumeration and device classification.

use schemars::JsonSchema;
use serde::Serialize;

/// What a connected serial device appears to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Esp32Target,
    PogoProbe,
    UnknownSerial,
}

/// Short description of a device, used in error details such as `AMBIGUOUS_DEVICE`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DeviceSummary {
    pub port: String,
    pub usb_serial: Option<String>,
    pub kind: DeviceKind,
}
