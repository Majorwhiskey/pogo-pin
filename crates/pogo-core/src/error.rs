//! Error types, stable error codes, and the exit code table (docs/agent-contract.md §6.4).

use schemars::JsonSchema;
use serde::Serialize;

use crate::discovery::DeviceSummary;

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
}

impl ErrorCode {
    /// Every code, for exhaustive tests and schema generation.
    pub const ALL: [ErrorCode; 15] = [
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
        }
    }

    /// Process exit code for this error (docs/agent-contract.md §6.4).
    pub fn exit_code(self) -> u8 {
        match self {
            ErrorCode::Internal | ErrorCode::NotImplemented => 1,
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

/// An operation could not run at all.
///
/// Outcomes such as a capture timeout or a target panic are not errors; they are
/// returned as data in `Ok(..)` and mapped to error codes by the frontend.
#[derive(Debug, thiserror::Error)]
pub enum PogoError {
    #[error("no device matched '{selector}'")]
    DeviceNotFound { selector: String },
    #[error("{} devices matched; pass --device to choose one", candidates.len())]
    AmbiguousDevice { candidates: Vec<DeviceSummary> },
    #[error("device is in use by pid {pid} ({command})")]
    DeviceBusy { pid: u32, command: String },
    #[error("permission denied opening {port}")]
    PermissionDenied { port: String },
    #[error("device I/O error: {0}")]
    DeviceIo(String),
    #[error("probe protocol error: {0}")]
    Protocol(String),
    #[error("probe speaks protocol v{probe_major}, this host needs v{host_major}")]
    ProtocolMismatch { host_major: u8, probe_major: u8 },
    #[error("refused by guardrail: {reason}")]
    GuardrailRefused { reason: String },
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
    #[error("internal error: {0}")]
    Internal(String),
}

impl PogoError {
    pub fn code(&self) -> ErrorCode {
        match self {
            PogoError::DeviceNotFound { .. } => ErrorCode::DeviceNotFound,
            PogoError::AmbiguousDevice { .. } => ErrorCode::AmbiguousDevice,
            PogoError::DeviceBusy { .. } => ErrorCode::DeviceBusy,
            PogoError::PermissionDenied { .. } => ErrorCode::PermissionDenied,
            PogoError::DeviceIo(_) => ErrorCode::DeviceIo,
            PogoError::Protocol(_) => ErrorCode::Protocol,
            PogoError::ProtocolMismatch { .. } => ErrorCode::ProtocolMismatch,
            PogoError::GuardrailRefused { .. } => ErrorCode::GuardrailRefused,
            PogoError::Config(_) => ErrorCode::ConfigInvalid,
            PogoError::NotImplemented(_) => ErrorCode::NotImplemented,
            PogoError::Internal(_) => ErrorCode::Internal,
        }
    }

    pub fn exit_code(&self) -> u8 {
        self.code().exit_code()
    }

    /// An actionable next step for the user or agent.
    pub fn hint(&self) -> Option<String> {
        let hint = match self {
            PogoError::DeviceNotFound { .. } => {
                "Run `pogo devices` to list connected boards, then pass --device <alias|usb_serial|port>.".to_string()
            }
            PogoError::AmbiguousDevice { .. } => {
                "Run `pogo devices` and pass --device <alias|usb_serial|port>.".to_string()
            }
            PogoError::DeviceBusy { pid, .. } => {
                format!("Wait for pid {pid} to finish, or pass --wait-lock <duration>.")
            }
            PogoError::PermissionDenied { .. } => permission_hint().to_string(),
            PogoError::DeviceIo(_) => {
                "Check the USB cable and run `pogo devices`; the board may have reset or been unplugged.".to_string()
            }
            PogoError::Protocol(_) => {
                "Unplug and replug the probe, then run `pogo probe info`.".to_string()
            }
            PogoError::ProtocolMismatch { host_major, .. } => format!(
                "Flash probe firmware that speaks protocol v{host_major} (see docs/probe-firmware.md §8.7)."
            ),
            PogoError::GuardrailRefused { .. } => {
                "See docs/guardrails.md for the guardrail and the flag that allows this, if any.".to_string()
            }
            PogoError::Config(_) => {
                "Run `pogo config validate` to locate the problem.".to_string()
            }
            PogoError::NotImplemented(_) => {
                "This command is planned but not built yet; see docs/roadmap.md.".to_string()
            }
            PogoError::Internal(_) => return None,
        };
        Some(hint)
    }

    /// Structured extra information for the JSON envelope's `error.details`.
    pub fn details(&self) -> Option<serde_json::Value> {
        match self {
            PogoError::AmbiguousDevice { candidates } => {
                Some(serde_json::json!({ "candidates": candidates }))
            }
            PogoError::DeviceBusy { pid, command } => {
                Some(serde_json::json!({ "pid": pid, "command": command }))
            }
            PogoError::PermissionDenied { port } => Some(serde_json::json!({ "port": port })),
            PogoError::ProtocolMismatch {
                host_major,
                probe_major,
            } => Some(serde_json::json!({
                "host_protocol_major": host_major,
                "probe_protocol_major": probe_major,
            })),
            _ => None,
        }
    }
}

fn permission_hint() -> &'static str {
    if cfg!(target_os = "linux") {
        "Add yourself to the serial group (sudo usermod -aG dialout $USER, or uucp on Arch), then log out and back in."
    } else if cfg!(target_os = "macos") {
        "Close other programs using the port (serial monitors, IDEs), then retry."
    } else {
        "Close other programs using the port and check the driver is installed."
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::DeviceKind;

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

    fn every_error() -> Vec<PogoError> {
        vec![
            PogoError::DeviceNotFound {
                selector: "target".into(),
            },
            PogoError::AmbiguousDevice {
                candidates: vec![DeviceSummary {
                    port: "/dev/ttyACM0".into(),
                    usb_serial: Some("F4:12:FA:9C:3A:10".into()),
                    kind: DeviceKind::Esp32Target,
                }],
            },
            PogoError::DeviceBusy {
                pid: 42,
                command: "pogo flash".into(),
            },
            PogoError::PermissionDenied {
                port: "/dev/ttyACM0".into(),
            },
            PogoError::DeviceIo("gone".into()),
            PogoError::Protocol("bad frame".into()),
            PogoError::ProtocolMismatch {
                host_major: 1,
                probe_major: 2,
            },
            PogoError::GuardrailRefused {
                reason: "eFuse".into(),
            },
            PogoError::Config("bad".into()),
            PogoError::NotImplemented("flash"),
            PogoError::Internal("oops".into()),
        ]
    }

    #[test]
    fn errors_map_to_expected_exit_codes() {
        let exits: Vec<u8> = every_error().iter().map(PogoError::exit_code).collect();
        assert_eq!(exits, [3, 3, 9, 5, 6, 6, 6, 8, 10, 1, 1]);
    }

    #[test]
    fn every_user_facing_error_has_a_hint() {
        for err in every_error() {
            if err.code() != ErrorCode::Internal {
                assert!(err.hint().is_some(), "{err:?} has no hint");
            }
        }
    }

    #[test]
    fn ambiguous_device_details_list_candidates() {
        let err = &every_error()[1];
        assert_eq!(
            err.to_string(),
            "1 devices matched; pass --device to choose one"
        );
        let details = err.details().unwrap();
        assert_eq!(details["candidates"][0]["kind"], "esp32_target");
        assert_eq!(details["candidates"][0]["usb_serial"], "F4:12:FA:9C:3A:10");
    }
}
