//! Kept value versions of running Flows on disk (ADR-016 phase D).
//!
//! A Flow keeps every replaced version while it lives (phase C). Once the
//! versions it keeps in memory pass the node's budget (`kept_memory_bytes`)
//! the end of the round hands them to the cold writer, in files of their
//! own that roll by size; once durable, each becomes a [`StoredValue`]
//! naming where it is, which holds its file until its last copy goes.
//! Reads take the bytes from the file (the page cache is the cache). A
//! snapshot names where they are in a section of its own; a cold image and
//! a portable image carry their bytes, and a logged after-image carries
//! only versions still in memory. A finished Flow's versions go to disk
//! with its own image instead.

use std::collections::HashSet;
use std::sync::Arc;

use wf_application::commands::flow::record::{Retained, RetainedKey, StoredValue, ValueFiles};
use wf_application::commands::queue::Partition;
use wf_domain::workflow::WorkflowStatus;
use wf_storage::cold::{ColdBatch, ColdStore, Location, checksum};

use super::super::image::StoredEntry;
use super::cold::{Answer, Slot};
use super::{Key, ShardState};

/// Kept versions smaller than this stay in memory: one on disk costs about
/// as much.
const MIN_STORED_BYTES: usize = 64;

/// The cold writer's group for kept versions: files that roll by size
/// only, since a running Flow has no removal hour.
const VALUES: u64 = u64::MAX;

/// A kept version handed to the writer, with where it is once durable
/// (`None` when it was not written).
pub(super) struct Moved {
    key: Key,
    kept: RetainedKey,
    /// The CRC32 of the bytes written, to find the same version.
    check: u32,
    location: Option<Location>,
}

/// Releases a kept version's hold on its cold file.
pub(super) struct Files(pub(super) Arc<ColdStore>);

impl ValueFiles for Files {
    fn release(&self, file: u64) {
        // An unused file not deleted now is tried again later.
        drop(self.0.release(file, 1));
    }
}

/// Where a kept version on disk is.
pub(crate) const fn location(stored: &StoredValue) -> Location {
    Location {
        file: stored.file,
        offset: stored.offset,
        length: stored.length,
    }
}

fn running(status: WorkflowStatus) -> bool {
    !matches!(
        status,
        WorkflowStatus::Completed | WorkflowStatus::Failed | WorkflowStatus::Cancelled
    )
}

impl ShardState {
    /// Notes a Flow that kept new versions this round.
    pub(super) fn kept_new(&mut self, key: &Key) {
        self.cold.kept.push(Arc::clone(key));
    }

    /// Hands the versions of Flows that keep more than the budget in memory
    /// to the cold writer.
    pub(super) fn spill(&mut self) {
        let mut noted = std::mem::take(&mut self.cold.kept);
        let mut seen = HashSet::new();
        noted.retain(|key| seen.insert(Arc::clone(key)));
        let mut images = Vec::new();
        let mut moving = Vec::new();
        let in_flight = &self.cold.moving;
        for key in noted {
            let Some(Slot::Hot(entry)) = self.records.get(&*key) else {
                continue;
            };
            let Some(meta) = entry.meta.as_deref() else {
                continue;
            };
            if !running(entry.record.status()) {
                continue;
            }
            // Versions already on their way stay out until answered.
            let sent = in_flight.get(&key);
            let resident: Vec<(&RetainedKey, &[u8])> = meta
                .retained
                .iter()
                .filter_map(|(kept, version)| match version {
                    Retained::Bytes(bytes)
                        if bytes.len() >= MIN_STORED_BYTES
                            && !meta.retained_unlogged.contains(kept)
                            && !sent.is_some_and(|sent| sent.contains(kept)) =>
                    {
                        Some((kept, bytes.as_slice()))
                    }
                    _ => None,
                })
                .collect();
            let total = resident.iter().fold(0_usize, |total, (_, bytes)| {
                total.saturating_add(bytes.len())
            });
            if u64::try_from(total).unwrap_or(u64::MAX) <= self.cold.budget {
                continue;
            }
            for (kept, bytes) in resident {
                images.push((VALUES, bytes.to_vec()));
                moving.push((Arc::clone(&key), kept.clone(), checksum(bytes)));
            }
        }
        if images.is_empty() {
            return;
        }
        for (key, kept, _) in &moving {
            self.cold
                .moving
                .entry(Arc::clone(key))
                .or_default()
                .insert(kept.clone());
        }
        let sender = self.cold.acks.0.clone();
        self.cold.in_flight = self.cold.in_flight.saturating_add(1);
        self.cold.queue.send(ColdBatch {
            images,
            done: Box::new(move |result| {
                // Not written: the versions stay in memory.
                let mut locations = result.map(Vec::into_iter).ok();
                let moved = moving
                    .into_iter()
                    .map(|(key, kept, check)| Moved {
                        key,
                        kept,
                        check,
                        location: locations.as_mut().and_then(Iterator::next),
                    })
                    .collect();
                // A stopped shard no longer needs its answer.
                drop(sender.send(Answer::Kept(moved)));
            }),
        });
    }

    /// Turns versions whose bytes are durable into references to disk; a
    /// version no longer held as it was gives its hold back.
    pub(super) fn settle_kept(&mut self, moved: Vec<Moved>) {
        for moved in moved {
            self.answered_kept(&moved.key, &moved.kept);
            let Some(location) = moved.location else {
                continue;
            };
            let version = match self.records.get_mut(&*moved.key) {
                Some(Slot::Hot(entry)) => entry
                    .meta
                    .as_deref_mut()
                    .and_then(|meta| meta.retained.get_mut(&moved.kept)),
                _ => None,
            };
            let same = |version: &Retained| {
                matches!(version, Retained::Bytes(bytes)
                    if checksum(bytes) == moved.check
                        && u32::try_from(bytes.len()) == Ok(location.length))
            };
            match version {
                Some(version) if same(version) => {
                    *version = Retained::Stored(Arc::new(StoredValue::new(
                        (location.file, location.offset, location.length),
                        Arc::clone(&self.cold.files),
                    )));
                }
                _ => drop(self.cold.store.release(location.file, 1)),
            }
        }
    }

    /// A version handed to the writer is no longer on its way.
    fn answered_kept(&mut self, key: &Key, kept: &RetainedKey) {
        let Some(sent) = self.cold.moving.get_mut(key) else {
            return;
        };
        sent.remove(kept);
        if sent.is_empty() {
            self.cold.moving.remove(key);
        }
    }

    /// The kept versions on disk of the Flows in memory, for a snapshot.
    pub(crate) fn kept_on_disk(&self) -> Vec<StoredEntry<'_>> {
        self.records
            .iter()
            .filter_map(|(key, slot)| match slot {
                Slot::Hot(entry) => entry.meta.as_deref().map(|meta| (key, meta)),
                Slot::Warm(..) | Slot::Cold(_) => None,
            })
            .flat_map(|(key, meta)| {
                meta.retained
                    .iter()
                    .filter_map(move |(kept, version)| match version {
                        Retained::Stored(on_disk) => {
                            Some(((&key.0, key.1.as_slice()), kept, location(on_disk)))
                        }
                        Retained::Bytes(_) => None,
                    })
            })
            .collect()
    }

    /// Restores a kept version on disk from a snapshot image, holding its
    /// file again.
    pub(crate) fn restore_stored(
        &mut self,
        partition: Partition,
        id: Vec<u8>,
        (kept, location): (RetainedKey, Location),
    ) {
        let Some(Slot::Hot(entry)) = self.records.get_mut(&(partition, id)) else {
            return;
        };
        let Some(meta) = entry.meta.as_deref_mut() else {
            return;
        };
        self.cold.store.hold(location.file, 1);
        meta.retained.insert(
            kept,
            Retained::Stored(Arc::new(StoredValue::new(
                (location.file, location.offset, location.length),
                Arc::clone(&self.cold.files),
            ))),
        );
    }
}
