//! Pure public protocol contracts, separate from persisted state and commands.
//!
//! M0 module scaffold; implementations follow the reviewed architecture.

#![forbid(unsafe_code)]

pub mod capability;
pub mod dto;
pub mod error;
pub mod operation;
pub mod scalar;
