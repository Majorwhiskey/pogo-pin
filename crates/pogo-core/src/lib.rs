//! Core library for pogo: all hardware logic, no printing, no process exit.

pub mod discovery;
pub mod envelope;
pub mod error;

pub use envelope::{Envelope, ErrorBody, Warning};
pub use error::{ErrorCode, PogoError};
