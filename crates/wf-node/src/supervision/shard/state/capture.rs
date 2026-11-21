//! Snapshot capture and restore of one shard's records (ADR-013).

use wf_application::commands::flow::history;

use super::super::image::{self, Restored};
use super::events::History;
use super::{Entry, Key, ShardState};

impl ShardState {
    /// Encodes every record in creation order; `None` when one cannot be
    /// imaged.
    pub(crate) fn image(&self, now: u64) -> Option<Vec<u8>> {
        let mut entries: Vec<(&Key, &Entry)> = self
            .records
            .iter()
            .map(|(key, entry)| (key, &**entry))
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
        Some(out)
    }

    /// Stores a record from a snapshot image, indexed like any other.
    pub(crate) fn restore(&mut self, restored: Restored) {
        let key = (restored.partition, restored.id);
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
