//! The group log's single writer thread.
//!
//! Appenders queue batches under one lock and wake the writer. The writer
//! takes everything queued, writes it with one write and one `sync_data`, and
//! then publishes the last sequence it covered. Waiters block on that
//! published sequence. A failed write or sync is published once and stops the
//! writer; later appends are refused.

use std::fs::File;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use super::{GroupLogError, block};

struct Queue {
    batches: Vec<Vec<Vec<u8>>>,
    last: u64,
    closing: bool,
}

#[derive(Clone, Copy)]
struct Durable {
    through: u64,
    failed: Option<GroupLogError>,
}

/// State shared by appenders, waiters and the writer.
pub(super) struct Shared {
    queue: Mutex<Queue>,
    queued: Condvar,
    durable: Mutex<Durable>,
    durable_changed: Condvar,
    syncs: AtomicU64,
    max_block_bytes: usize,
}

/// Critical sections only move values, so a poisoned lock still holds
/// consistent state.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    pub(super) fn new(max_block_bytes: usize) -> Self {
        Self {
            queue: Mutex::new(Queue {
                batches: Vec::new(),
                last: 0,
                closing: false,
            }),
            queued: Condvar::new(),
            durable: Mutex::new(Durable {
                through: 0,
                failed: None,
            }),
            durable_changed: Condvar::new(),
            syncs: AtomicU64::new(0),
            max_block_bytes,
        }
    }

    pub(super) fn enqueue(&self, batch: Vec<Vec<u8>>) -> Result<u64, GroupLogError> {
        let mut queue = lock(&self.queue);
        if queue.closing {
            return Err(GroupLogError::Closed);
        }
        if let Some(failure) = lock(&self.durable).failed {
            return Err(failure);
        }
        let sequence = queue
            .last
            .checked_add(1)
            .ok_or(GroupLogError::Unavailable)?;
        queue.last = sequence;
        queue.batches.push(batch);
        drop(queue);
        self.queued.notify_one();
        Ok(sequence)
    }

    pub(super) fn wait(&self, sequence: u64) -> Result<(), GroupLogError> {
        let mut durable = lock(&self.durable);
        loop {
            if durable.through >= sequence {
                return Ok(());
            }
            if let Some(failure) = durable.failed {
                return Err(failure);
            }
            durable = self
                .durable_changed
                .wait(durable)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    pub(super) fn close(&self) {
        lock(&self.queue).closing = true;
        self.queued.notify_one();
    }

    pub(super) fn syncs(&self) -> u64 {
        self.syncs.load(Ordering::Relaxed)
    }

    /// Waits for queued batches; returns them, the last sequence among them
    /// and whether the log is closing.
    fn take(&self) -> (Vec<Vec<Vec<u8>>>, u64, bool) {
        let mut queue = lock(&self.queue);
        while queue.batches.is_empty() && !queue.closing {
            queue = self
                .queued
                .wait(queue)
                .unwrap_or_else(PoisonError::into_inner);
        }
        (
            std::mem::take(&mut queue.batches),
            queue.last,
            queue.closing,
        )
    }

    fn publish(&self, through: u64, outcome: Result<(), GroupLogError>) {
        let mut durable = lock(&self.durable);
        match outcome {
            Ok(()) => durable.through = through,
            Err(failure) => durable.failed = Some(failure),
        }
        drop(durable);
        self.durable_changed.notify_all();
    }

    fn write(&self, file: &mut File, batches: &[Vec<Vec<u8>>]) -> Result<(), GroupLogError> {
        let bytes = block::encode(batches, self.max_block_bytes)?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_data())
            .map_err(|_| GroupLogError::Uncertain)?;
        self.syncs.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

/// Starts the writer thread, which owns `file` until the log closes.
pub(super) fn spawn(shared: Arc<Shared>, file: File) -> Result<JoinHandle<()>, GroupLogError> {
    std::thread::Builder::new()
        .name("wf-group-log".into())
        .spawn(move || run(&shared, file))
        .map_err(|_| GroupLogError::Unavailable)
}

fn run(shared: &Shared, mut file: File) {
    loop {
        let (batches, last, closing) = shared.take();
        if batches.is_empty() {
            return;
        }
        let outcome = shared.write(&mut file, &batches);
        let failed = outcome.is_err();
        shared.publish(last, outcome);
        if failed || closing {
            return;
        }
    }
}
