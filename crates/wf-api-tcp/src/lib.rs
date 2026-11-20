//! TCP adapter over the shared application facade, speaking FSNP v1 (ADR-011).
//!
//! The frame header, typed values and the workflow request and reply codecs
//! exist and are tested against captured Elixir frames. No listener or session
//! exists yet.
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
