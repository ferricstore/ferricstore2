//! Composition, bounded committed startup recovery and supervision boundaries.
//!
//! Startup recovery joins the concrete durable log and state stores. Runtime
//! wiring and supervision remain incomplete; this crate starts no workflow service.
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
