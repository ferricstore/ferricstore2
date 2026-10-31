//! Pure governing-owner decisions; durable and remote authority remain external.

pub mod catalog;
pub mod credit;
pub mod identified;
pub mod view;

/// Public credit caller normalization, separate from private mutation preparation.
pub mod caller;

/// Durable-release value planning; external effects remain in adapters.
pub mod release;
