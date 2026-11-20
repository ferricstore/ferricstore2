//! Supervised runtime threads (ADR-011): one thread per shard and one
//! completion thread that releases replies after the log sync.

pub(crate) mod completer;
pub(crate) mod shard;

use std::thread::JoinHandle;

/// Starts a named runtime thread.
pub(crate) fn spawn(name: String, body: impl FnOnce() + Send + 'static) -> Option<JoinHandle<()>> {
    std::thread::Builder::new().name(name).spawn(body).ok()
}
