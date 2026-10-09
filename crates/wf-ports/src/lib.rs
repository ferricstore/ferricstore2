//! Neutral effect and client contract boundaries. No backend types are exposed.
//!
//! M0 module scaffold; implementations follow the reviewed architecture.

#![forbid(unsafe_code)]

pub mod blob;
pub mod client;
pub mod clock;
pub mod consensus;
pub mod id_source;
pub mod log;
pub mod notification;
pub mod projection;
pub mod snapshot;
pub mod state;
