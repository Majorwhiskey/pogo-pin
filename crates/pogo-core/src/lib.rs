//! Core library for pogo: all hardware logic, no printing, no process exit.

pub mod config;
pub mod deadline;
pub mod discovery;
pub mod envelope;
pub mod error;
pub mod lock;
pub mod reset;
pub mod schema;
pub mod serial;
pub mod transport;

pub use envelope::{Envelope, ErrorBody, Warning};
pub use error::{ErrorCode, PogoError};
