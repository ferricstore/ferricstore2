//! TCP adapter boundaries over the shared application facade. No listener exists yet.
//!
//! M0 module scaffold; implementations follow the reviewed architecture.
//! Client adapters cannot import raw effect ports:
//! ```compile_fail,E0432
//! use wf_ports::consensus;
//! ```
//!

#![forbid(unsafe_code)]

mod codec;
pub mod frame;
mod listener;
pub mod request;
pub mod response;
pub mod session;
