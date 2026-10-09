//! Pure binary identity, source-compatible partition encoding, and checked values.
//!
//! Storage identity bounds and composed API-key admission are distinct checks.
//! Time and fences are explicit values; this crate has no runtime dependencies
//! and performs no clock, storage, routing, or workflow effects.

#![forbid(unsafe_code)]

pub mod bounds;
pub mod error;
pub mod identity;
pub mod version;
