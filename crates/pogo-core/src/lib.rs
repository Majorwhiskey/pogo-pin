//! Core library for pogo: all hardware logic, no printing, no process exit.

pub mod config;
pub mod discovery;
pub mod envelope;
pub mod error;
pub mod schema;

pub use envelope::{Envelope, ErrorBody, Warning};
pub use error::{ErrorCode, PogoError};
