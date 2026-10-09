//! Consensus adapter boundaries. No Raft implementation is included yet.
//!
//! M0 module scaffold; implementations follow the reviewed architecture.

#![forbid(unsafe_code)]

pub mod adapter;
pub mod apply;
pub mod membership;
pub mod read;
