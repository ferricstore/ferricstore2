//! The completion thread (ADR-011): waits until each round's changes are
//! durable, then releases its replies. Shard threads never wait for a sync,
//! so they keep applying while the log writer groups their appends.

use std::sync::mpsc::Receiver;

use wf_application::commands::queue::ServiceError;
use wf_storage::log::GroupLog;

/// A reply waiting for durability; called with the sync's outcome.
pub(crate) type Finish = Box<dyn FnOnce(Result<(), ServiceError>) + Send>;

/// Replies of one shard round and the log sequence covering its changes.
pub(crate) struct Completion {
    pub(crate) sequence: u64,
    pub(crate) finishes: Vec<Finish>,
}

/// The completion thread's loop; returns when every shard has stopped.
pub(crate) fn run(log: &GroupLog, completions: &Receiver<Completion>) {
    while let Ok(completion) = completions.recv() {
        let durable = log
            .wait_durable(completion.sequence)
            .map_err(|_| ServiceError::Unavailable);
        for finish in completion.finishes {
            finish(durable.clone());
        }
    }
}
