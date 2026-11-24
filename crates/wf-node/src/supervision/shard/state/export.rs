//! A shard's portable image (ADR-018 phase 3): every Flow as a record,
//! finished Flows read back from their cold files, so another member can
//! install it without this one's files; and its installation into a fresh
//! shard, whose finished Flows then move to its own cold files.

use super::super::image::{self, Kept};
use super::cold::Slot;
use super::{Entry, Key, ShardState};

/// A Flow for the image: held in memory, or read from its cold file.
enum Held<'a> {
    Here(&'a Entry),
    Loaded(Box<Entry>),
}

impl Held<'_> {
    fn entry(&self) -> &Entry {
        match self {
            Self::Here(entry) => entry,
            Self::Loaded(entry) => entry,
        }
    }
}

impl ShardState {
    /// The shard as a portable image; `None` when a Flow cannot be read or
    /// imaged.
    pub(crate) fn export(&self, now: u64) -> Option<Vec<u8>> {
        let mut entries: Vec<(&Key, Held<'_>)> = Vec::with_capacity(self.records.len());
        for (key, slot) in &self.records {
            let held = match slot {
                Slot::Hot(entry) | Slot::Warm(entry, _) => Held::Here(entry),
                Slot::Cold(stub) => Held::Loaded(self.load(stub).ok()?),
            };
            entries.push((key, held));
        }
        entries.sort_unstable_by_key(|(_, held)| held.entry().sequence);
        let mut out = Vec::new();
        out.extend_from_slice(&u64::try_from(entries.len()).ok()?.to_be_bytes());
        for (key, held) in entries {
            let entry = held.entry();
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
                Kept::All(self.cold_store()),
            )?;
        }
        self.encode_tail(now, &mut out)?;
        image::encode_stubs(&mut out, &[])?;
        self.encode_holdings(&mut out)?;
        image::encode_stored(&mut out, &[])?;
        Some(out)
    }

    /// Replaces the shard with the one `bytes` images; `None`, leaving the
    /// shard as it was, when the image does not decode or restore.
    pub(crate) fn install(&mut self, bytes: &[u8]) -> Option<()> {
        let decoded = image::decode(bytes)?;
        let mut fresh = Self::new(self.retention_ms, self.cold.handles());
        for value in decoded.shared {
            fresh.store_shared(
                value.partition,
                value.reference,
                value.bytes,
                value.expires_at,
            );
        }
        for (flow_type, generation, policy) in decoded.policies {
            fresh.restore_policy(&flow_type, generation, &policy).ok()?;
        }
        for (kind, name, value) in decoded.governance {
            fresh.restore_governance(kind, &name, Some(value)).ok()?;
        }
        for restored in decoded.records {
            fresh.restore(restored);
        }
        for stub in decoded.stubs {
            fresh.restore_stub(stub);
        }
        for holding in decoded.holdings {
            fresh.restore_holdings(holding.partition, holding.id, &holding.refs);
        }
        for kept in decoded.stored {
            fresh.restore_stored(kept.partition, kept.id, (kept.kept, kept.location));
        }
        *self = fresh;
        Some(())
    }
}
