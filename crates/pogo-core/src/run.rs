//! `data` for `pogo run`: flash, then reset and capture from the first boot byte.

use schemars::JsonSchema;
use serde::Serialize;

use crate::discovery::ResolvedDevice;
use crate::flash::{FlashPlan, FlashReport};
use crate::serial::capture::CaptureResult;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RunData {
    pub device: ResolvedDevice,
    /// `null` with `--no-flash`.
    pub flash: Option<FlashReport>,
    /// `null` unless `--dry-run`.
    pub plan: Option<FlashPlan>,
    /// `null` with `--dry-run`.
    pub capture: Option<CaptureResult>,
}
