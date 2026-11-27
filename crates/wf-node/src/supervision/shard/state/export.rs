//! A shard's portable image (ADR-018 phase 3): every Flow as a record,
//! finished Flows read back from their cold files, so another member can
//! install it without this one's files; and its installation into a fresh
//! shard, whose finished Flows move to its own cold files as they arrive.
//! Both go a block at a time (`image::blocks`), so neither holds the
//! shard's whole image: the export reads one finished Flow at a time, and
//! the install moves finished Flows to cold files every [`COOL_EVERY`]
//! records.

use std::time::Duration;

use super::super::Part;
use super::super::image::{self, Kept, blocks};
use super::cold::Slot;
use super::{Entry, Key, ShardState};

/// Records an install restores between moving finished Flows to disk.
const COOL_EVERY: usize = 1024;
/// How long an install waits for those Flows' images to be durable.
const COOL_WAIT: Duration = Duration::from_secs(60);

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

/// When a Flow was created, for creation order.
fn sequence(slot: &Slot) -> u64 {
    match slot {
        Slot::Hot(entry) | Slot::Warm(entry, _) => entry.sequence,
        Slot::Cold(stub) => stub.sequence,
    }
}

impl ShardState {
    /// Sends the shard to `send` as blocks: its head, each Flow in creation
    /// order, an empty block, then its trailer; `None` when a Flow cannot be
    /// read or imaged, or `send` refuses a block (the writer stopped).
    pub(crate) fn export_to(&self, now: u64, send: &mut dyn FnMut(Vec<u8>) -> bool) -> Option<()> {
        let mut first = Vec::new();
        self.encode_tail(now, &mut first)?;
        send(first).then_some(())?;
        let mut slots: Vec<(&Key, &Slot)> = self.records.iter().collect();
        slots.sort_unstable_by_key(|(_, slot)| sequence(slot));
        for (key, slot) in slots {
            let held = match slot {
                Slot::Hot(entry) | Slot::Warm(entry, _) => Held::Here(entry),
                Slot::Cold(stub) => Held::Loaded(self.load(stub).ok()?),
            };
            let entry = held.entry();
            let events = entry.history.events();
            let mut block = Vec::new();
            image::encode_record(
                &mut block,
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
            send(block).then_some(())?;
        }
        send(Vec::new()).then_some(())?;
        let mut trailer = Vec::new();
        image::encode_stubs(&mut trailer, &[])?;
        self.encode_holdings(&mut trailer)?;
        image::encode_stored(&mut trailer, &[])?;
        send(trailer).then_some(())
    }

    /// Builds a fresh shard from the blocks `next` yields, as
    /// [`Self::export_to`] sends them, and replaces this one with it once
    /// they end in a commit; `None`, leaving the shard as it was, when a
    /// block does not decode or restore, or the blocks stop first.
    pub(crate) fn install_from(&mut self, next: &mut dyn FnMut() -> Option<Part>) -> Option<()> {
        let mut block = || match next()? {
            Part::Block(block) => Some(block),
            Part::Commit => None,
        };
        let mut fresh = Self::new(self.retention_ms, self.cold.handles());
        let head = blocks::head(&block()?)?;
        for value in head.shared {
            fresh.store_shared(
                value.partition,
                value.reference,
                value.bytes,
                value.expires_at,
            );
        }
        for (flow_type, generation, policy) in head.policies {
            fresh.restore_policy(&flow_type, generation, &policy).ok()?;
        }
        for (kind, name, value) in head.governance {
            fresh.restore_governance(kind, &name, Some(value)).ok()?;
        }
        let mut restored: usize = 0;
        loop {
            let record = block()?;
            if record.is_empty() {
                break;
            }
            fresh.restore(blocks::record(&record)?);
            restored = restored.wrapping_add(1);
            if restored.is_multiple_of(COOL_EVERY) {
                fresh.cool();
                fresh.settle_cold(COOL_WAIT);
            }
        }
        let trailer = blocks::trailer(&block()?)?;
        for stub in trailer.stubs {
            fresh.restore_stub(stub);
        }
        for holding in trailer.holdings {
            fresh.restore_holdings(holding.partition, holding.id, &holding.refs);
        }
        for kept in trailer.stored {
            fresh.restore_stored(kept.partition, kept.id, (kept.kept, kept.location));
        }
        if !matches!(next(), Some(Part::Commit)) {
            return None;
        }
        *self = fresh;
        Some(())
    }
}
