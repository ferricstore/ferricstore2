//! Storage-neutral checkpoint verification and consensus adapter boundaries.
//!
//! Bounded checkpoint verification and neutral control-entry apply are implemented.
//! Node composes membership audit and committed replay through neutral ports.
//! The Raft runtime, snapshot integration and read barriers remain incomplete.

#![forbid(unsafe_code)]

pub mod adapter;
pub mod apply;
pub mod membership;
pub mod read;
pub mod recovery;
