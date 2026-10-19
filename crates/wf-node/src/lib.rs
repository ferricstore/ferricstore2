//! Composition and supervision boundary. This scaffold starts no workflow service.
//!
//! M0 module scaffold; implementations follow the reviewed architecture.
//! Worker transport modules are private to the worker crate:
//! ```compile_fail,E0603
//! use wf_worker::client;
//! ```
//!

#![forbid(unsafe_code)]

pub mod composition;
pub mod configuration;
pub mod recovery;
pub mod shutdown;
pub mod supervision;
