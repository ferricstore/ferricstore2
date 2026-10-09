//! Test-only infrastructure. This crate implements no workflow behavior.
//!
//! Time is advanced explicitly. Fault waits use bounded wall time only to fail
//! stalled test coordination; they never determine workflow deadlines.

#![forbid(unsafe_code)]

pub mod clock;
pub mod fault;
pub mod resources;
