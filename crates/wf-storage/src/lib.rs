//! Scoped synchronous durable log and transactional workflow state adapters.
//!
//! The reviewed redb backend persists authority, indexes, retained outcomes and
//! applied control/membership and indexed state snapshots. Blob adapters and
//! snapshot-aware runtime integration remain incomplete.

#![forbid(unsafe_code)]

pub mod blob;
pub mod log;
pub mod snapshot;
pub mod state;
pub mod transaction;
