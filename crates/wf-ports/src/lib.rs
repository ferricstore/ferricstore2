//! Neutral effect/client contracts and private prepared-command codecs.
//!
//! Versioned encoders and decoders use immutable byte and work profiles.
//! Durable storage, consensus and live client adapters retain their reviewed
//! implementation gates.

#![forbid(unsafe_code)]

pub mod blob;
pub mod client;
pub mod clock;
pub mod consensus;
pub mod id_source;
pub mod log;
pub mod notification;
pub mod projection;
pub mod snapshot;
pub mod state;
