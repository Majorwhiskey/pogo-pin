//! JSON Schemas for command envelopes (docs/agent-contract.md §6).

use schemars::schema_for;
use serde_json::Value;

use crate::config::{ConfigPathResult, ConfigShowResult, ConfigValidateResult};
use crate::discovery::DevicesResult;
use crate::envelope::Envelope;
use crate::flash::{FlashData, InfoData};
use crate::reset::ResetData;
use crate::run::RunData;
use crate::serial::send::SerialSendData;
use crate::serial::session::SerialCaptureData;

/// Commands whose `data` shape is defined, in the order `schema --all` emits them.
pub const COMMANDS_WITH_SCHEMA: &[&str] = &[
    "devices",
    "info",
    "flash",
    "reset",
    "run",
    "serial.capture",
    "serial.send",
    "config.show",
    "config.path",
    "config.validate",
];

/// The JSON Schema for `command`'s envelope, or `None` if it has no defined data shape.
pub fn envelope_schema(command: &str) -> Option<Value> {
    let mut schema = match command {
        "devices" => schema_for!(Envelope<DevicesResult>),
        "info" => schema_for!(Envelope<InfoData>),
        "flash" => schema_for!(Envelope<FlashData>),
        "reset" => schema_for!(Envelope<ResetData>),
        "run" => schema_for!(Envelope<RunData>),
        "serial.capture" => schema_for!(Envelope<SerialCaptureData>),
        "serial.send" => schema_for!(Envelope<SerialSendData>),
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
        assert!(envelope_schema("i2c.scan").is_none());
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
