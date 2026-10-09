//! Application command/query facades over explicit effect ports.
//!
//! M0 module scaffold; implementations follow the reviewed architecture.

#![forbid(unsafe_code)]

mod admission;
mod authorization;
pub mod commands;
pub mod error;
mod intent;
pub mod queries;
mod reconciliation;
mod routing;
