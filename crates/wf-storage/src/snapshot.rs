//! Durable immutable snapshot data and privately validated restore custody,
//! and the queue node's snapshot files (ADR-013).

pub mod files;

pub use crate::state::snapshot::RedbSnapshotStage;
