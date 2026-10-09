//! Worker orchestration boundaries with private transport implementations.
//!
//! M0 module scaffold; implementations follow the reviewed architecture.

#![forbid(unsafe_code)]

mod client;
pub mod handler;
pub mod shutdown;
pub mod worker;
