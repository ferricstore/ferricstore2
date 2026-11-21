//! Supervised runtime threads (ADR-011, ADR-013): one thread per shard, one
//! completion thread that releases replies after the log sync, the waiter
//! thread for blocking claims and the snapshot thread.

pub(crate) mod completer;
pub mod health;
pub(crate) mod scheduler;
pub(crate) mod shard;
pub(crate) mod snapshots;
pub(crate) mod waiters;

use std::thread::JoinHandle;

/// A Tokio runtime for a replicated group's Raft and peer traffic
/// (ADR-018), on threads named `name`.
pub(crate) fn runtime(name: &str) -> Option<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name(name)
        .enable_io()
        .enable_time()
        .build()
        .ok()
}

/// Starts a named runtime thread.
pub(crate) fn spawn(name: String, body: impl FnOnce() + Send + 'static) -> Option<JoinHandle<()>> {
    std::thread::Builder::new().name(name).spawn(body).ok()
}
