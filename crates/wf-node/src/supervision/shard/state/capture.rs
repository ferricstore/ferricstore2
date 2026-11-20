//! Snapshot capture and restore of one shard's records (ADR-013).

use super::super::image::{self, Restored};
use super::{Entry, Key, ShardState};

impl ShardState {
    /// Encodes every record in creation order; `None` when one cannot be
    /// imaged.
    pub(crate) fn image(&self) -> Option<Vec<u8>> {
        let mut entries: Vec<(&Key, &Entry)> = self.records.iter().collect();
        entries.sort_unstable_by_key(|(_, entry)| entry.sequence);
        let mut out = Vec::new();
        out.extend_from_slice(&u64::try_from(entries.len()).ok()?.to_be_bytes());
        for (key, entry) in entries {
            image::encode_record(
                &mut out,
                &key.0,
                &key.1,
                entry.priority,
                (&entry.record, entry.meta.as_deref()),
            )?;
        }
        Some(out)
    }

    /// Stores a record from a snapshot image, indexed like any other.
    pub(crate) fn restore(&mut self, restored: Restored) {
        self.put(
            (restored.partition, restored.id),
            restored.record,
            restored.priority,
            restored.meta.map(Box::new),
        );
    }
}
