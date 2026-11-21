//! Snapshot capture and restore of one shard's records (ADR-013).

use wf_application::commands::flow::history;

use super::super::image::{self, Restored};
use super::cold::{Slot, Stub};
use super::events::History;
use super::{Entry, Key, ShardState, flow_key};

impl ShardState {
    /// Encodes every record in creation order; `None` when one cannot be
    /// imaged.
    pub(crate) fn image(&self, now: u64) -> Option<Vec<u8>> {
        let mut entries: Vec<(&Key, &Entry)> = self
            .records
            .iter()
            .filter_map(|(key, slot)| match slot {
                Slot::Hot(entry) => Some((key, &**entry)),
                Slot::Warm(..) | Slot::Cold(_) => None,
            })
            .collect();
        entries.sort_unstable_by_key(|(_, entry)| entry.sequence);
        let mut out = Vec::new();
        out.extend_from_slice(&u64::try_from(entries.len()).ok()?.to_be_bytes());
        for (key, entry) in entries {
            let events = entry.history.events();
            image::encode_record(
                &mut out,
                &key.0,
                &key.1,
                entry.priority,
                (
                    &entry.record,
                    entry.meta.as_deref(),
                    Some(events.as_slice()),
                ),
            )?;
        }
        let shared: Vec<image::SharedEntry<'_>> = self
            .shared_values(now)
            .into_iter()
            .map(|(reference, value)| {
                (
                    &value.partition,
                    reference.as_slice(),
                    value.bytes.as_slice(),
                    value.expires_at,
                )
            })
            .collect();
        image::encode_shared(&mut out, &shared)?;
        let mut policies = Vec::new();
        for installed in self.policies() {
            let mut encoded = Vec::new();
            installed.policy.term().encode(&mut encoded)?;
            policies.push((
                installed.flow_type.as_slice(),
                installed.generation,
                encoded,
            ));
        }
        image::encode_policies(&mut out, &policies)?;
        let records: Vec<(u8, &[u8], &[u8])> = self.governance_records().collect();
        image::encode_governance(&mut out, records.into_iter())?;
        let mut stubs: Vec<(&Key, &Stub)> = self
            .records
            .iter()
            .filter_map(|(key, slot)| match slot {
                Slot::Warm(_, stub) | Slot::Cold(stub) => Some((key, stub)),
                Slot::Hot(_) => None,
            })
            .collect();
        stubs.sort_unstable_by_key(|(_, stub)| stub.sequence);
        let stubs: Vec<image::StubEntry<'_>> = stubs
            .into_iter()
            .map(|(key, stub)| {
                (
                    (&key.0, key.1.as_slice()),
                    stub.location,
                    (&*stub.flow_type, &*stub.state),
                    (stub.updated_at, stub.priority, stub.removal),
                    (u64::from(stub.history), u64::from(stub.values)),
                )
            })
            .collect();
        image::encode_stubs(&mut out, &stubs)?;
        Some(out)
    }

    /// Restores a finished Flow on disk from a snapshot image (ADR-016).
    pub(crate) fn restore_stub(&mut self, image: image::StubImage) {
        let key = flow_key(image.partition, image.id);
        let sequence = self.sequence();
        self.catalog_insert_at(&key, (&image.flow_type, &image.state, image.updated_at));
        self.expiries.insert((image.removal, sequence), key.clone());
        self.cold_store().hold(image.location.file, 1);
        let stub = self.stub(
            image.location,
            (&image.flow_type, &image.state),
            (image.updated_at, sequence, image.priority, image.removal),
            (image.history, image.values),
        );
        self.records.insert(key, Slot::Cold(stub));
    }

    /// Stores a record from a snapshot image, indexed like any other.
    pub(crate) fn restore(&mut self, restored: Restored) {
        let key = flow_key(restored.partition, restored.id);
        let meta = restored.meta.map(Box::new);
        match restored.history {
            Some(events) => {
                let cap = meta
                    .as_ref()
                    .map_or(history::default_cap(), |meta| meta.history_max_events);
                let events = History::of(&events, cap);
                self.reinstate(key, (restored.record, restored.priority, meta, events));
            }
            None => self.put(key, (restored.record, restored.priority, meta), None),
        }
    }
}
