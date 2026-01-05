//! Virtual Scroll - Platform-agnostic virtual scroll state machine
//!
//! This crate provides a pure state machine for managing virtual scroll pagination.
//! It has NO external dependencies - consumers pass scroll metrics as input and
//! receive selection strings as output.
//!
//! See SPEC.md for full documentation.

mod metrics;
mod query;
mod manager;

pub use metrics::*;
pub use query::*;
pub use manager::*;
