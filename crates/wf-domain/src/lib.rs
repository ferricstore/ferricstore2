//! Pure deterministic workflow boundaries. Workflow behavior is not implemented yet.
//!
//! M0 module scaffold; implementations follow the reviewed architecture.

#![forbid(unsafe_code)]

pub mod claim;
pub mod history;
pub mod lease;
pub mod lineage;
pub mod policy;
pub mod retention;
pub mod retry;
pub mod schedule;
pub mod signal;
pub mod value;
pub mod workflow;
