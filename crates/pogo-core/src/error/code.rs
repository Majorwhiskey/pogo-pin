//! Stable error codes and the exit code table (docs/agent-contract.md §6.4).

use schemars::JsonSchema;
use serde::Serialize;

/// Stable machine-readable error code. Serialized as `SCREAMING_SNAKE_CASE`.
///
/// Changing a code's name or exit code is a breaking change to the agent contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    Internal,
    NotImplemented,
    Usage,
    DeviceNotFound,
    AmbiguousDevice,
    Timeout,
    PermissionDenied,
    DeviceIo,
    Protocol,
    ProtocolMismatch,
    TargetFault,
    GuardrailRefused,
    DeviceBusy,
    ConfigInvalid,
    Interrupted,
    /// `pogo doctor` found a failing check.
    CheckFailed,
}

impl ErrorCode {
    /// Every code, for exhaustive tests and schema generation.
    pub const ALL: [ErrorCode; 16] = [
        ErrorCode::Internal,
        ErrorCode::NotImplemented,
        ErrorCode::Usage,
        ErrorCode::DeviceNotFound,
        ErrorCode::AmbiguousDevice,
        ErrorCode::Timeout,
        ErrorCode::PermissionDenied,
        ErrorCode::DeviceIo,
        ErrorCode::Protocol,
        ErrorCode::ProtocolMismatch,
        ErrorCode::TargetFault,
        ErrorCode::GuardrailRefused,
        ErrorCode::DeviceBusy,
        ErrorCode::ConfigInvalid,
        ErrorCode::Interrupted,
        ErrorCode::CheckFailed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Internal => "INTERNAL",
            ErrorCode::NotImplemented => "NOT_IMPLEMENTED",
            ErrorCode::Usage => "USAGE",
            ErrorCode::DeviceNotFound => "DEVICE_NOT_FOUND",
            ErrorCode::AmbiguousDevice => "AMBIGUOUS_DEVICE",
            ErrorCode::Timeout => "TIMEOUT",
            ErrorCode::PermissionDenied => "PERMISSION_DENIED",
            ErrorCode::DeviceIo => "DEVICE_IO",
            ErrorCode::Protocol => "PROTOCOL",
            ErrorCode::ProtocolMismatch => "PROTOCOL_MISMATCH",
            ErrorCode::TargetFault => "TARGET_FAULT",
            ErrorCode::GuardrailRefused => "GUARDRAIL_REFUSED",
            ErrorCode::DeviceBusy => "DEVICE_BUSY",
            ErrorCode::ConfigInvalid => "CONFIG_INVALID",
            ErrorCode::Interrupted => "INTERRUPTED",
            ErrorCode::CheckFailed => "CHECK_FAILED",
        }
    }

    /// Process exit code for this error (docs/agent-contract.md §6.4).
    pub fn exit_code(self) -> u8 {
        match self {
            ErrorCode::Internal | ErrorCode::NotImplemented | ErrorCode::CheckFailed => 1,
            ErrorCode::Usage => 2,
            ErrorCode::DeviceNotFound | ErrorCode::AmbiguousDevice => 3,
            ErrorCode::Timeout => 4,
            ErrorCode::PermissionDenied => 5,
            ErrorCode::DeviceIo | ErrorCode::Protocol | ErrorCode::ProtocolMismatch => 6,
            ErrorCode::TargetFault => 7,
            ErrorCode::GuardrailRefused => 8,
            ErrorCode::DeviceBusy => 9,
            ErrorCode::ConfigInvalid => 10,
            ErrorCode::Interrupted => 130,
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The §6.4 table. Changing any row is a breaking change.
    #[test]
    fn exit_codes_match_spec_table() {
        let expected = [
            (ErrorCode::Internal, "INTERNAL", 1),
            (ErrorCode::NotImplemented, "NOT_IMPLEMENTED", 1),
            (ErrorCode::Usage, "USAGE", 2),
            (ErrorCode::DeviceNotFound, "DEVICE_NOT_FOUND", 3),
            (ErrorCode::AmbiguousDevice, "AMBIGUOUS_DEVICE", 3),
            (ErrorCode::Timeout, "TIMEOUT", 4),
            (ErrorCode::PermissionDenied, "PERMISSION_DENIED", 5),
            (ErrorCode::DeviceIo, "DEVICE_IO", 6),
            (ErrorCode::Protocol, "PROTOCOL", 6),
            (ErrorCode::ProtocolMismatch, "PROTOCOL_MISMATCH", 6),
            (ErrorCode::TargetFault, "TARGET_FAULT", 7),
            (ErrorCode::GuardrailRefused, "GUARDRAIL_REFUSED", 8),
            (ErrorCode::DeviceBusy, "DEVICE_BUSY", 9),
            (ErrorCode::ConfigInvalid, "CONFIG_INVALID", 10),
            (ErrorCode::Interrupted, "INTERRUPTED", 130),
            (ErrorCode::CheckFailed, "CHECK_FAILED", 1),
        ];
        assert_eq!(expected.len(), ErrorCode::ALL.len());
        for (code, name, exit) in expected {
            assert_eq!(code.as_str(), name);
            assert_eq!(code.exit_code(), exit, "{name}");
        }
    }

    #[test]
    fn serde_name_matches_as_str() {
        for code in ErrorCode::ALL {
            let json = serde_json::to_value(code).unwrap();
            assert_eq!(json, serde_json::Value::String(code.as_str().to_string()));
        }
    }
}
