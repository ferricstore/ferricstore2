//! The group log's single writer thread.
//!
//! Appenders queue items under one lock and wake the writer; while the queued
//! records hold the byte limit or more, an appender waits for the writer to
//! take them (ADR-017). The writer takes everything queued and handles it in
//! order: consecutive batches are written
//! with one write and one sync, a rotation starts a new segment after the
//! earlier batches are durable, and a removal deletes segments before a
//! checkpoint. After each step it publishes the last sequence handled; waiters
//! block on that. A failed write, sync or segment creation is published once
//! and stops the writer, and later appends are refused. A failed removal is
//! reported to its caller only: the extra segments are still valid.
//!
//! Dropping the log lets the writer handle what is queued, then stop.
//! Closing it (its owner stopping) is final at once: queued items are
//! dropped unwritten, appenders and waiters are refused, and the writer
//! stops before its next write, rotation or removal; only a write already
//! under way finishes.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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

impl Item {
    /// Record bytes it adds to the queue, with each record's length prefix.
    fn bytes(&self) -> usize {
        match self {
            Self::Batch(records) => records.iter().fold(0_usize, |total, record| {
                total.saturating_add(record.len()).saturating_add(4)
            }),
            Self::Rotate | Self::RemoveBefore(_) => 0,
        }
    }
}

struct Queue {
    items: Vec<(u64, Item)>,
    /// Record bytes of the queued items.
    bytes: usize,
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
    /// Signalled when the writer takes the queue, the log closes or fails.
    drained: Condvar,
    max_queued_bytes: usize,
    durable: Mutex<Durable>,
    durable_changed: Condvar,
    syncs: AtomicU64,
    /// The segment appends go to.
    segment: AtomicU64,
    /// Closed for good: the writer touches the segments no more.
    closed: AtomicBool,
}

/// Critical sections only move values, so a poisoned lock still holds
/// consistent state.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    pub(super) fn new(segment: u64, max_queued_bytes: usize) -> Self {
        Self {
            queue: Mutex::new(Queue {
                items: Vec::new(),
                bytes: 0,
                last: 0,
                closing: false,
            }),
            queued: Condvar::new(),
            drained: Condvar::new(),
            max_queued_bytes,
            durable: Mutex::new(Durable {
                through: 0,
                failed: None,
                outcomes: BTreeMap::new(),
            }),
            durable_changed: Condvar::new(),
            syncs: AtomicU64::new(0),
            segment: AtomicU64::new(segment),
            closed: AtomicBool::new(false),
        }
    }

    pub(super) fn enqueue(&self, item: Item) -> Result<u64, GroupLogError> {
        let size = item.bytes();
        let mut queue = lock(&self.queue);
        loop {
            if queue.closing {
                return Err(GroupLogError::Closed);
            }
            if let Some(failure) = lock(&self.durable).failed {
                return Err(failure);
            }
            if size == 0 || queue.bytes < self.max_queued_bytes.max(1) {
                break;
            }
            queue = self
                .drained
                .wait(queue)
                .unwrap_or_else(PoisonError::into_inner);
        }
        queue.bytes = queue.bytes.saturating_add(size);
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

    /// Lets the writer handle what is queued, then stop.
    pub(super) fn finish(&self) {
        lock(&self.queue).closing = true;
        self.queued.notify_one();
        self.drained.notify_all();
    }

    /// Closes the log for good: what is queued is dropped unwritten, every
    /// appender and waiter not yet answered is refused, and the writer
    /// stops before its next step.
    pub(super) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        let mut queue = lock(&self.queue);
        queue.closing = true;
        queue.items.clear();
        queue.bytes = 0;
        drop(queue);
        self.fail(GroupLogError::Closed);
        self.queued.notify_one();
    }

    pub(super) fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Refuses a step of the writer once the log is closed.
    fn open(&self) -> Result<(), GroupLogError> {
        if self.closed() {
            return Err(GroupLogError::Closed);
        }
        Ok(())
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
        queue.bytes = 0;
        let taken = (std::mem::take(&mut queue.items), queue.closing);
        drop(queue);
        self.drained.notify_all();
        taken
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

    /// Publishes the first failure; a later one (the close after a failed
    /// write) leaves it.
    fn fail(&self, failure: GroupLogError) {
        let mut durable = lock(&self.durable);
        if durable.failed.is_none() {
            durable.failed = Some(failure);
        }
        drop(durable);
        self.durable_changed.notify_all();
        // Taking the queue lock orders this after any appender's check, so a
        // waiting appender sees the failure.
        drop(lock(&self.queue));
        self.drained.notify_all();
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
        shared.open()?;
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
        shared.open()?;
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

    fn remove_before(&mut self, shared: &Shared, before: u64) -> Result<u64, GroupLogError> {
        if before > self.segment {
            return Err(GroupLogError::InvalidCheckpoint);
        }
        while self.oldest < before {
            shared.open()?;
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
                    self.remove_before(shared, before)
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
