//! Supervised runtime threads (ADR-011, ADR-013): one thread per shard, one
//! completion thread that releases replies after the log sync, the waiter
//! thread for blocking claims and the snapshot thread.

pub(crate) mod completer;
pub(crate) mod shard;
pub(crate) mod snapshots;
pub(crate) mod waiters;

use std::thread::JoinHandle;

/// Starts a named runtime thread.
pub(crate) fn spawn(name: String, body: impl FnOnce() + Send + 'static) -> Option<JoinHandle<()>> {
    std::thread::Builder::new().name(name).spawn(body).ok()
}
