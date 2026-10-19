//! Storage-neutral checkpoint verification and consensus adapter boundaries.
//!
//! Bounded startup verification is implemented; the Raft runtime, membership
//! recovery, snapshots and committed-suffix replay remain incomplete.

#![forbid(unsafe_code)]

pub mod adapter;
pub mod apply;
pub mod membership;
pub mod read;
pub mod recovery;
