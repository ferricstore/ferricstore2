//! Durable storage adapter boundaries. No engine has been selected by this scaffold.
//!
//! M0 module scaffold; implementations follow the reviewed architecture.

#![forbid(unsafe_code)]

pub mod blob;
pub mod log;
pub mod snapshot;
pub mod state;
pub mod transaction;
