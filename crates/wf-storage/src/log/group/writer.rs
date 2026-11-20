//! The group log's single writer thread.
//!
//! Appenders queue items under one lock and wake the writer. The writer takes
//! everything queued and handles it in order: consecutive batches are written
//! with one write and one sync, a rotation starts a new segment after the
//! earlier batches are durable, and a removal deletes segments before a
//! checkpoint. After each step it publishes the last sequence handled; waiters
//! block on that. A failed write, sync or segment creation is published once
//! and stops the writer, and later appends are refused. A failed removal is
//! reported to its caller only: the extra segments are still valid.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use super::storage::{SegmentStorage, SegmentWriter};
use super::{GroupLogError, block};

/// One queued request, handled in sequence order.
pub(super) enum Item {
    Batch(Vec<Vec<u8>>),
    Rotate,
    RemoveBefore(u64),
}

struct Queue {
    items: Vec<(u64, Item)>,
    last: u64,
    closing: bool,
}

struct Durable {
    through: u64,
    failed: Option<GroupLogError>,
    /// Results of rotations (the new segment) and removals, by sequence.
    outcomes: BTreeMap<u64, Result<u64, GroupLogError>>,
}

/// State shared by appenders, waiters and the writer.
pub(super) struct Shared {
    queue: Mutex<Queue>,
    queued: Condvar,
    durable: Mutex<Durable>,
    durable_changed: Condvar,
    syncs: AtomicU64,
    /// The segment appends go to.
    segment: AtomicU64,
}

/// Critical sections only move values, so a poisoned lock still holds
/// consistent state.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    pub(super) fn new(segment: u64) -> Self {
        Self {
            queue: Mutex::new(Queue {
                items: Vec::new(),
                last: 0,
                closing: false,
            }),
            queued: Condvar::new(),
            durable: Mutex::new(Durable {
                through: 0,
                failed: None,
                outcomes: BTreeMap::new(),
            }),
            durable_changed: Condvar::new(),
            syncs: AtomicU64::new(0),
            segment: AtomicU64::new(segment),
        }
    }

    pub(super) fn enqueue(&self, item: Item) -> Result<u64, GroupLogError> {
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
        queue.items.push((sequence, item));
        drop(queue);
        self.queued.notify_one();
        Ok(sequence)
    }

    pub(super) fn wait(&self, sequence: u64) -> Result<(), GroupLogError> {
        self.wait_locked(sequence).map(drop)
    }

    /// Waits for a rotation or removal and returns its result.
    pub(super) fn outcome(&self, sequence: u64) -> Result<u64, GroupLogError> {
        let mut durable = self.wait_locked(sequence)?;
        durable
            .outcomes
            .remove(&sequence)
            .unwrap_or(Err(GroupLogError::Unavailable))
    }

    fn wait_locked(&self, sequence: u64) -> Result<MutexGuard<'_, Durable>, GroupLogError> {
        let mut durable = lock(&self.durable);
        loop {
            if durable.through >= sequence {
                return Ok(durable);
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

    pub(super) fn segment(&self) -> u64 {
        self.segment.load(Ordering::Relaxed)
    }

    /// Waits for queued items; returns them and whether the log is closing.
    fn take(&self) -> (Vec<(u64, Item)>, bool) {
        let mut queue = lock(&self.queue);
        while queue.items.is_empty() && !queue.closing {
            queue = self
                .queued
                .wait(queue)
                .unwrap_or_else(PoisonError::into_inner);
        }
        (std::mem::take(&mut queue.items), queue.closing)
    }

    fn publish(&self, through: u64, outcome: Option<Result<u64, GroupLogError>>) {
        let mut durable = lock(&self.durable);
        durable.through = through;
        if let Some(outcome) = outcome {
            durable.outcomes.insert(through, outcome);
        }
        drop(durable);
        self.durable_changed.notify_all();
    }

    fn fail(&self, failure: GroupLogError) {
        lock(&self.durable).failed = Some(failure);
        self.durable_changed.notify_all();
    }
}

/// The writer thread's view of the segments.
pub(super) struct Writer {
    pub(super) storage: Arc<dyn SegmentStorage>,
    pub(super) current: Box<dyn SegmentWriter>,
    pub(super) segment: u64,
    pub(super) length: u64,
    pub(super) oldest: u64,
    pub(super) max_block_bytes: usize,
    pub(super) max_segment_bytes: u64,
}

impl Writer {
    fn rotate(&mut self, shared: &Shared) -> Result<u64, GroupLogError> {
        let next = self
            .segment
            .checked_add(1)
            .ok_or(GroupLogError::Uncertain)?;
        self.current = self
            .storage
            .create(next)
            .map_err(|_| GroupLogError::Uncertain)?;
        self.segment = next;
        self.length = 0;
        shared.segment.store(next, Ordering::Relaxed);
        Ok(next)
    }

    fn write(&mut self, shared: &Shared, batches: &[Vec<Vec<u8>>]) -> Result<(), GroupLogError> {
        let bytes = block::encode(batches, self.max_block_bytes)?;
        let added = u64::try_from(bytes.len()).map_err(|_| GroupLogError::TooLarge)?;
        let grown = self
            .length
            .checked_add(added)
            .ok_or(GroupLogError::TooLarge)?;
        if self.length > 0 && grown > self.max_segment_bytes {
            self.rotate(shared)?;
        }
        self.current
            .append(&bytes)
            .and_then(|()| self.current.sync())
            .map_err(|_| GroupLogError::Uncertain)?;
        self.length = self
            .length
            .checked_add(added)
            .ok_or(GroupLogError::Uncertain)?;
        shared.syncs.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn remove_before(&mut self, before: u64) -> Result<u64, GroupLogError> {
        if before > self.segment {
            return Err(GroupLogError::InvalidCheckpoint);
        }
        while self.oldest < before {
            self.storage
                .remove(self.oldest)
                .map_err(|_| GroupLogError::Unavailable)?;
            self.oldest = self
                .oldest
                .checked_add(1)
                .ok_or(GroupLogError::Unavailable)?;
        }
        Ok(before)
    }

    /// Writes queued batches, if any, and publishes them as durable.
    fn flush(
        &mut self,
        shared: &Shared,
        pending: &mut Vec<Vec<Vec<u8>>>,
        last: u64,
    ) -> Result<(), GroupLogError> {
        if pending.is_empty() {
            return Ok(());
        }
        self.write(shared, pending)?;
        pending.clear();
        shared.publish(last, None);
        Ok(())
    }

    /// Handles one round of queued items in order.
    fn handle(&mut self, shared: &Shared, items: Vec<(u64, Item)>) -> Result<(), GroupLogError> {
        let mut pending = Vec::new();
        let mut last = 0;
        for (sequence, item) in items {
            let outcome = match item {
                Item::Batch(batch) => {
                    pending.push(batch);
                    last = sequence;
                    continue;
                }
                Item::Rotate => {
                    self.flush(shared, &mut pending, last)?;
                    Ok(self.rotate(shared)?)
                }
                Item::RemoveBefore(before) => {
                    self.flush(shared, &mut pending, last)?;
                    self.remove_before(before)
                }
            };
            shared.publish(sequence, Some(outcome));
        }
        self.flush(shared, &mut pending, last)
    }
}

/// Starts the writer thread, which owns the segments until the log closes.
pub(super) fn spawn(shared: Arc<Shared>, writer: Writer) -> Result<JoinHandle<()>, GroupLogError> {
    std::thread::Builder::new()
        .name("wf-group-log".into())
        .spawn(move || run(&shared, writer))
        .map_err(|_| GroupLogError::Unavailable)
}

fn run(shared: &Shared, mut writer: Writer) {
    loop {
        let (items, closing) = shared.take();
        if items.is_empty() {
            return;
        }
        if let Err(failure) = writer.handle(shared, items) {
            shared.fail(failure);
            return;
        }
        if closing {
            return;
        }
    }
}
