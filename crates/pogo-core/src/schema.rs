//! JSON Schemas for command envelopes (docs/agent-contract.md §6).

use schemars::schema_for;
use serde_json::Value;

use crate::config::{ConfigPathResult, ConfigShowResult, ConfigValidateResult};
use crate::discovery::DevicesResult;
use crate::doctor::DoctorData;
use crate::envelope::Envelope;
use crate::flash::{FlashData, InfoData};
use crate::ops::gpio::{GpioGetData, GpioPulseData, GpioSetData};
use crate::ops::i2c::{I2cReadData, I2cScanData, I2cWriteData};
use crate::ops::power::{PowerCycleData, PowerSwitchData};
use crate::ops::power_measure::PowerMeasureData;
use crate::ops::probe::{ProbeInfoData, SelftestData};
use crate::ops::setup::SetupClaudeData;
use crate::reset::ResetData;
use crate::run::RunData;
use crate::serial::send::SerialSendData;
use crate::serial::session::SerialCaptureData;
use crate::suite::SuiteData;

/// Commands whose `data` shape is defined, in the order `schema --all` emits them.
pub const COMMANDS_WITH_SCHEMA: &[&str] = &[
    "devices",
    "doctor",
    "info",
    "flash",
    "reset",
    "run",
    "test",
    "setup.claude",
    "serial.capture",
    "serial.send",
    "i2c.scan",
    "i2c.read",
    "i2c.write",
    "gpio.get",
    "gpio.set",
    "gpio.pulse",
    "power.measure",
    "power.on",
    "power.off",
    "power.cycle",
    "probe.info",
    "probe.selftest",
    "config.show",
    "config.path",
    "config.validate",
];

/// The JSON Schema for `command`'s envelope, or `None` if it has no defined data shape.
pub fn envelope_schema(command: &str) -> Option<Value> {
    let mut schema = match command {
        "devices" => schema_for!(Envelope<DevicesResult>),
        "doctor" => schema_for!(Envelope<DoctorData>),
        "info" => schema_for!(Envelope<InfoData>),
        "flash" => schema_for!(Envelope<FlashData>),
        "reset" => schema_for!(Envelope<ResetData>),
        "run" => schema_for!(Envelope<RunData>),
        "test" => schema_for!(Envelope<SuiteData>),
        "setup.claude" => schema_for!(Envelope<SetupClaudeData>),
        "serial.capture" => schema_for!(Envelope<SerialCaptureData>),
        "serial.send" => schema_for!(Envelope<SerialSendData>),
        "i2c.scan" => schema_for!(Envelope<I2cScanData>),
        "i2c.read" => schema_for!(Envelope<I2cReadData>),
        "i2c.write" => schema_for!(Envelope<I2cWriteData>),
        "gpio.get" => schema_for!(Envelope<GpioGetData>),
        "gpio.set" => schema_for!(Envelope<GpioSetData>),
        "gpio.pulse" => schema_for!(Envelope<GpioPulseData>),
        "power.measure" => schema_for!(Envelope<PowerMeasureData>),
        "power.on" | "power.off" => schema_for!(Envelope<PowerSwitchData>),
        "power.cycle" => schema_for!(Envelope<PowerCycleData>),
        "probe.info" => schema_for!(Envelope<ProbeInfoData>),
        "probe.selftest" => schema_for!(Envelope<SelftestData>),
        "config.show" => schema_for!(Envelope<ConfigShowResult>),
        "config.path" => schema_for!(Envelope<ConfigPathResult>),
        "config.validate" => schema_for!(Envelope<ConfigValidateResult>),
        _ => return None,
    };
    schema.insert(
        "title".into(),
        Value::String(format!("pogo {command} envelope")),
    );
    Some(schema.to_value())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_command_has_a_schema() {
        for cmd in COMMANDS_WITH_SCHEMA {
            let s = envelope_schema(cmd).unwrap();
            assert_eq!(s["title"], format!("pogo {cmd} envelope"));
            assert!(s["properties"]["data"].is_object(), "{cmd}");
        }
        assert!(envelope_schema("nope").is_none());
    }

    #[test]
    fn committed_schemas_match_code() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
        for cmd in COMMANDS_WITH_SCHEMA {
            let path = dir.join(format!("{cmd}.json"));
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let committed: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(
                committed,
                envelope_schema(cmd).unwrap(),
                "{cmd}: schemas/ is stale; run `pogo schema --all --out schemas/`"
            );
        }
    }
}
