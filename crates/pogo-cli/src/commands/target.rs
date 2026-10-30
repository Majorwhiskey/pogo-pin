//! From global flags to a core `Selection`, and from a core `OpResult` to a `Reply`.

use std::path::PathBuf;

use pogo_core::ops::{OpResult, Selection};
use pogo_core::{ErrorBody, ErrorCode};
use serde::Serialize;

use crate::cli::GlobalArgs;
use crate::output::{Render, Reply, Style};

pub fn selection(global: &GlobalArgs) -> Selection {
    Selection {
        config: global.config.clone(),
        device: global.device.clone(),
        wait_lock: global.wait_lock,
        // Test hook: read a recorded session instead of a device (docs/testing.md).
        replay: std::env::var_os("POGO_REPLAY").map(PathBuf::from),
    }
}

/// Data (possibly partial), error, and warnings become one reply.
pub fn reply<T: Serialize + Render>(result: OpResult<T>, style: &Style) -> Reply {
    let mut reply = match (&result.data, result.error) {
        (Some(data), Some(e)) => Reply::ok(data, style).with_error(e),
        (Some(data), None) => Reply::ok(data, style),
        (None, Some(e)) => Reply::error(e),
        (None, None) => Reply::error(ErrorBody::new(
            ErrorCode::Internal,
            "command produced neither data nor an error",
        )),
    };
    reply.warnings.extend(result.warnings);
    reply
}
