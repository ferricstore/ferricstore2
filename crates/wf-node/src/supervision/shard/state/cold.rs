//! Finished Flows on disk (ADR-016).
//!
//! A Flow that turns terminal is encoded at the end of its round (record,
//! fields and history: the after-image encoding) and handed to the cold
//! writer thread; once the image is durable its slot keeps only a
//! [`Stub`]: where the image is and what the indexes need without reading
//! it. Until then it stays in memory, so the shard never waits on a file. A command that names a cold Flow loads
//! it for the round ([`ShardState::warm`]); unchanged, it goes back to its
//! stub when the round ends, so reads cost no write. Scans read cold Flows
//! without keeping them ([`ShardState::seen`]). Its indexes (catalog,
//! removal schedule) stay as they were: they are computed from fields the
//! stub keeps.

use std::collections::HashSet;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use wf_application::commands::queue::ServiceError;
use wf_storage::cold::{ColdQueue, ColdStore, Location};

use super::super::image;
use super::events::History;
use super::{Entry, Key, Map, Parts, ShardState};

/// Flows removed in the same hour share files.
const HOUR_MS: u64 = 3_600_000;

mod settle;

use settle::Ack;

/// Where a Flow is.
pub(super) enum Slot {
    /// In memory.
    Hot(Box<Entry>),
    /// Loaded from its cold file for this round and unchanged.
    Warm(Box<Entry>, Stub),
    /// On disk.
    Cold(Stub),
}

/// What a finished Flow keeps in memory.
#[derive(Clone, Debug)]
pub(super) struct Stub {
    pub(super) location: Location,
    pub(super) flow_type: Arc<[u8]>,
    pub(super) state: Arc<[u8]>,
    pub(super) updated_at: u64,
    pub(super) sequence: u64,
    pub(super) priority: u8,
    /// When the retention sweep removes it.
    pub(super) removal: u64,
    /// History events and stored values, for cleanup counts.
    pub(super) history: u32,
    pub(super) values: u32,
}

/// A Flow read for a scan: borrowed when in memory, decoded when cold.
pub(super) enum Seen<'a> {
    Here(&'a Entry),
    Loaded(Box<Entry>),
}

impl Deref for Seen<'_> {
    type Target = Entry;

    fn deref(&self) -> &Entry {
        match self {
            Self::Here(entry) => entry,
            Self::Loaded(entry) => entry,
        }
    }
}

/// One shard's link to the cold files and the Flows of this round to move
/// or restore.
pub(super) struct Cold {
    store: Arc<ColdStore>,
    queue: Arc<ColdQueue>,
    acks: (Sender<Vec<Ack>>, Receiver<Vec<Ack>>),
    /// Batches sent and not answered yet.
    in_flight: usize,
    /// Flows that turned terminal this round.
    pub(super) demote: Vec<Key>,
    /// Flows loaded for this round.
    warmed: Vec<Key>,
    /// One shared copy of each type and state name.
    names: Map<Vec<u8>, Arc<[u8]>>,
}

impl Cold {
    pub(super) fn new(store: Arc<ColdStore>, queue: Arc<ColdQueue>) -> Self {
        Self {
            store,
            queue,
            acks: channel(),
            in_flight: 0,
            demote: Vec::new(),
            warmed: Vec::new(),
            names: Map::default(),
        }
    }

    fn name(&mut self, bytes: &[u8]) -> Arc<[u8]> {
        if let Some(name) = self.names.get(bytes) {
            return Arc::clone(name);
        }
        let name: Arc<[u8]> = Arc::from(bytes);
        self.names.insert(bytes.to_vec(), Arc::clone(&name));
        name
    }
}

/// A cold image: the record's after-image without history, as a `u32`
/// length and bytes, then the packed history as it is in memory (event
/// count and newest offset as `u32`, then its bytes), so a Flow goes to
/// disk without decoding and encoding its events again.
fn encode(key: &Key, entry: &Entry) -> Option<Vec<u8>> {
    let record = image::encode_one(
        &key.0,
        &key.1,
        entry.priority,
        (&entry.record, entry.meta.as_deref(), None),
    )?;
    let (packed, count, last) = entry.history.packed();
    let mut out = Vec::with_capacity(record.len().saturating_add(packed.len()).saturating_add(12));
    out.extend_from_slice(&u32::try_from(record.len()).ok()?.to_be_bytes());
    out.extend_from_slice(&record);
    out.extend_from_slice(&count.to_be_bytes());
    out.extend_from_slice(&last.to_be_bytes());
    out.extend_from_slice(packed);
    Some(out)
}

/// The Flow a cold image holds, with the creation `sequence` its stub kept.
fn decode(bytes: &[u8], sequence: u64) -> Option<Box<Entry>> {
    let (length, rest) = bytes.split_first_chunk::<4>()?;
    let length = usize::try_from(u32::from_be_bytes(*length)).ok()?;
    let (record, rest) = rest.split_at_checked(length)?;
    let (count, rest) = rest.split_first_chunk::<4>()?;
    let (last, packed) = rest.split_first_chunk::<4>()?;
    let restored = image::decode_one(record)?;
    let history = History::from_packed(
        packed.to_vec(),
        u32::from_be_bytes(*count),
        u32::from_be_bytes(*last),
    )?;
    Some(Box::new(Entry {
        record: restored.record,
        priority: restored.priority,
        sequence,
        meta: restored.meta.map(Box::new),
        history,
    }))
}

/// The stored values of a Flow: payload, result, error and named values.
pub(super) fn values(entry: &Entry) -> u32 {
    let record = &entry.record;
    let named = entry.meta.as_ref().map_or(0, |meta| meta.values.len());
    let count = [record.payload(), record.result(), record.error()]
        .iter()
        .flatten()
        .count()
        .saturating_add(named);
    u32::try_from(count).unwrap_or(u32::MAX)
}

impl ShardState {
    /// The Flow under `key` when it is in memory.
    pub(super) fn hot(&self, key: &Parts) -> Option<&Entry> {
        match self.records.get(key)? {
            Slot::Hot(entry) | Slot::Warm(entry, _) => Some(entry),
            Slot::Cold(_) => None,
        }
    }

    /// The stored key and Flow under `key` when it is in memory.
    pub(super) fn hot_kv(&self, key: &Parts) -> Option<(&Key, &Entry)> {
        match self.records.get_key_value(key)? {
            (key, Slot::Hot(entry) | Slot::Warm(entry, _)) => Some((key, entry)),
            (_, Slot::Cold(_)) => None,
        }
    }

    /// The Flow under `key`, read from its cold file if needed; a cold
    /// image that cannot be read reads as missing.
    pub(super) fn seen(&self, key: &Parts) -> Option<Seen<'_>> {
        match self.records.get(key)? {
            Slot::Hot(entry) | Slot::Warm(entry, _) => Some(Seen::Here(entry)),
            Slot::Cold(stub) => self.load(stub).ok().map(Seen::Loaded),
        }
    }

    /// [`Self::seen`] with the stored key.
    pub(super) fn seen_kv(&self, key: &Parts) -> Option<(&Key, Seen<'_>)> {
        let (stored, slot) = self.records.get_key_value(key)?;
        let seen = match slot {
            Slot::Hot(entry) | Slot::Warm(entry, _) => Seen::Here(entry),
            Slot::Cold(stub) => Seen::Loaded(self.load(stub).ok()?),
        };
        Some((stored, seen))
    }

    fn load(&self, stub: &Stub) -> Result<Box<Entry>, ServiceError> {
        let bytes = self
            .cold
            .store
            .read(stub.location)
            .map_err(|_| ServiceError::Unavailable)?;
        decode(&bytes, stub.sequence).ok_or(ServiceError::Unavailable)
    }

    /// Loads the cold Flow under `key`, if any, for this round.
    ///
    /// # Errors
    /// Its image cannot be read.
    pub(super) fn warm(&mut self, key: &Parts) -> Result<(), ServiceError> {
        let Some((stored, Slot::Cold(stub))) = self.records.get_key_value(key) else {
            return Ok(());
        };
        let (stored, stub) = (stored.clone(), stub.clone());
        let entry = self.load(&stub)?;
        self.records.insert(stored.clone(), Slot::Warm(entry, stub));
        self.cold.warmed.push(stored);
        Ok(())
    }

    /// Takes the Flow under `key` out of its slot to change it; its cold
    /// image, if any, is no longer used.
    pub(super) fn take_slot(&mut self, key: &Parts) -> Option<Box<Entry>> {
        let (stored, slot) = self.records.remove_entry(key)?;
        match slot {
            Slot::Hot(entry) => Some(entry),
            Slot::Warm(entry, stub) => {
                // The removal the stub was given, in case the retention
                // setting changed since.
                self.expiries.remove(&(stub.removal, stub.sequence));
                self.forget(&stub);
                Some(entry)
            }
            Slot::Cold(stub) => {
                // Callers load first; a read failing now drops the Flow's
                // indexes rather than leaving them on no slot.
                let loaded = self.load(&stub).ok();
                if loaded.is_none() {
                    self.unindex_stub(&stored, &stub);
                }
                self.expiries.remove(&(stub.removal, stub.sequence));
                self.forget(&stub);
                loaded
            }
        }
    }

    fn forget(&self, stub: &Stub) {
        // A failed delete leaves the file for the next restart's sweep.
        drop(self.cold.store.release(stub.location.file, 1));
    }

    fn unindex_stub(&mut self, key: &Key, stub: &Stub) {
        self.catalog_remove_at(key, (&stub.flow_type, &stub.state, stub.updated_at));
        self.expiries.remove(&(stub.removal, stub.sequence));
    }

    /// Removes a cold Flow at the end of its retention.
    pub(super) fn remove_cold(&mut self, key: &Parts) {
        if let Some((stored, Slot::Cold(stub))) = self.records.remove_entry(key) {
            self.unindex_stub(&stored, &stub);
            self.forget(&stub);
        }
    }

    /// A stub with shared copies of its type and state names.
    pub(super) fn stub(
        &mut self,
        location: Location,
        (flow_type, state): (&[u8], &[u8]),
        (updated_at, sequence, priority, removal): (u64, u64, u8, u64),
        (history, values): (u64, u64),
    ) -> Stub {
        Stub {
            location,
            flow_type: self.cold.name(flow_type),
            state: self.cold.name(state),
            updated_at,
            sequence,
            priority,
            removal,
            history: u32::try_from(history).unwrap_or(u32::MAX),
            values: u32::try_from(values).unwrap_or(u32::MAX),
        }
    }

    pub(super) fn cold_store(&self) -> &ColdStore {
        &self.cold.store
    }

    /// The cold files this shard's stubs name.
    pub(crate) fn cold_files(&self) -> HashSet<u64> {
        self.records
            .values()
            .filter_map(|slot| match slot {
                Slot::Cold(stub) | Slot::Warm(_, stub) => Some(stub.location.file),
                Slot::Hot(_) => None,
            })
            .collect()
    }
}
