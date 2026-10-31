//! Application boundaries and pure canonical caller-intent preparation.
//!
//! The commands facade prepares bounded identities and delegates original-first
//! Claim inquiry/retry through explicit capabilities. Production authorization,
//! operation registries and supervised service adapters retain their gates.

#![forbid(unsafe_code)]

mod admission;
mod authorization;
pub mod commands;
pub mod error;
pub mod governance;
mod intent;
pub mod queries;
mod reconciliation;
mod routing;
