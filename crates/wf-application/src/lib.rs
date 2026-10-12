//! Application boundaries and pure canonical caller-intent preparation.
//!
//! The commands facade prepares bounded identities from explicit typed inputs.
//! Service authorization, operation registries and durable submission retain
//! their reviewed implementation gates.

#![forbid(unsafe_code)]

mod admission;
mod authorization;
pub mod commands;
pub mod error;
mod intent;
pub mod queries;
mod reconciliation;
mod routing;
