//! The entry log's notifier thread: it waits for each append's sequence to
//! be durable, in order, and calls its completion, so appenders never block
//! on a sync. Closing the queue ends the thread after the queued waits.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use super::super::group::{GroupLog, GroupLogError};

/// Called once with the outcome of an append's sync.
pub type Durable = Box<dyn FnOnce(Result<(), GroupLogError>) + Send>;

pub(super) struct Notifier {
    queue: Option<Sender<(u64, Durable)>>,
    thread: Option<JoinHandle<()>>,
}

impl Notifier {
    pub(super) fn start(log: Arc<GroupLog>) -> Result<Self, GroupLogError> {
        let (queue, waits) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("wf-entry-notifier".into())
            .spawn(move || run(&log, &waits))
            .map_err(|_| GroupLogError::Unavailable)?;
        Ok(Self {
            queue: Some(queue),
            thread: Some(thread),
        })
    }

    /// Calls `done` once `sequence` is durable or the log failed.
    pub(super) fn notify(&self, sequence: u64, done: Durable) {
        match &self.queue {
            Some(queue) => {
                if let Err(returned) = queue.send((sequence, done)) {
                    (returned.0.1)(Err(GroupLogError::Closed));
                }
            }
            None => done(Err(GroupLogError::Closed)),
        }
    }
}

fn run(log: &GroupLog, waits: &Receiver<(u64, Durable)>) {
    while let Ok((sequence, done)) = waits.recv() {
        done(log.wait_durable(sequence));
    }
}

impl Drop for Notifier {
    fn drop(&mut self) {
        self.queue = None;
        if let Some(thread) = self.thread.take() {
            // The thread only returns; a join error has nothing to report.
            let _joined = thread.join();
        }
    }
}
