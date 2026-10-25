//! Core library for pogo: all hardware logic, no printing, no process exit.

pub mod discovery;
pub mod error;

pub use error::{ErrorCode, PogoError};
