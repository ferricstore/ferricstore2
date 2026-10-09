//! HTTP adapter boundaries over the shared application facade. No listener exists yet.
//!
//! M0 module scaffold; implementations follow the reviewed architecture.
//! Client adapters cannot import raw effect ports:
//! ```compile_fail,E0432
//! use wf_ports::consensus;
//! ```
//!

#![forbid(unsafe_code)]

pub mod error;
mod listener;
mod middleware;
pub mod request;
pub mod response;
