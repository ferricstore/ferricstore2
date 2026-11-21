//! Values other Flows hold (audit gap G10): a value a Flow attaches by
//! reference stays readable while that Flow exists, even after the Flow
//! that owns it is removed.
//!
//! A Flow holds every reference it has attached since it was created
//! (history keeps the earlier ones), to a named value or a payload, result
//! or error of another Flow on this shard (attachments share the holder's
//! slot: [`check_slot`]). When retention removes an owner whose values are
//! held, they move to the shard's shared values under their references; a
//! value moved there goes when retention removes its last holder. Both steps
//! run inside the logged removal, so replay and every replica repeat them.

use wf_application::commands::flow::value::{self, ValueRef};
use wf_application::commands::queue::{Partition, ServiceError, slot_for_tag};

use super::super::ShardState;
use super::super::key::{Key, Map, Parts};
use super::Shared;

/// The references Flows on this shard hold to values they do not own.
#[derive(Default)]
pub(crate) struct Held {
    /// Each holding Flow's references, in the order it attached them.
    by_flow: Map<Key, Vec<Vec<u8>>>,
    /// How many Flows hold each reference.
    holders: Map<Vec<u8>, u32>,
    /// How many held references each owner (hash tag, id) has.
    owners: Map<(Vec<u8>, Vec<u8>), u32>,
}

impl Held {
    /// One reference to `owner`'s values is no longer held.
    fn forget_owner(&mut self, owner: &(Vec<u8>, Vec<u8>)) {
        let Some(count) = self.owners.get_mut(owner) else {
            return;
        };
        *count = count.saturating_sub(1);
        if *count == 0 {
            self.owners.remove(owner);
        }
    }
}

/// The owner (hash tag, id) of a reference to a Flow's value; `None` for a
/// shared value's, which lives on its own.
fn owner(reference: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let parsed = value::parse_ref(reference)?;
    match parsed.target {
        ValueRef::Named { owner, .. } => Some((parsed.tag, owner)),
        ValueRef::Flow { id, .. } => Some((parsed.tag, id)),
        ValueRef::Shared => None,
    }
}

/// Refuses an attached reference outside the Flow's slot: values are read
/// on the Flow's own shard under any shard count.
///
/// # Errors
/// `CROSSSLOT` for a reference whose hash tag routes to another slot.
pub(in crate::supervision::shard::state) fn check_slot(
    partition: &Partition,
    reference: &[u8],
) -> Result<(), ServiceError> {
    let slot = value::parse_ref(reference).map(|parsed| slot_for_tag(&parsed.tag));
    if slot.is_some_and(|slot| slot != partition.slot()) {
        return Err(ServiceError::rejected(
            "CROSSSLOT Flow shared value refs must hash to the Flow slot".to_owned(),
        ));
    }
    Ok(())
}

impl ShardState {
    /// Records that the Flow under `key` holds `refs`; its own values and
    /// shared values are not held.
    pub(in crate::supervision::shard::state) fn hold<'a>(
        &mut self,
        key: &Key,
        refs: impl IntoIterator<Item = &'a Vec<u8>>,
    ) {
        let tag = key.0.tag().into_bytes();
        for reference in refs {
            let Some(owner) = owner(reference) else {
                continue;
            };
            if owner.0 == tag && owner.1 == key.1 {
                continue;
            }
            let held = self.held.by_flow.entry(key.clone()).or_default();
            if held.contains(reference) {
                continue;
            }
            held.push(reference.clone());
            let holders = self.held.holders.entry(reference.clone()).or_default();
            *holders = holders.saturating_add(1);
            let references = self.held.owners.entry(owner).or_default();
            *references = references.saturating_add(1);
        }
    }

    /// The Flow under `key` is gone: it holds nothing, and a moved value
    /// no Flow holds any more goes too.
    pub(in crate::supervision::shard::state) fn release(&mut self, key: &Parts) {
        let Some(refs) = self.held.by_flow.remove(key) else {
            return;
        };
        for reference in refs {
            if let Some(owner) = owner(&reference) {
                self.held.forget_owner(&owner);
            }
            let Some(count) = self.held.holders.get_mut(&reference) else {
                continue;
            };
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.held.holders.remove(&reference);
                self.shared.remove(&reference);
            }
        }
    }

    /// Before retention removes the Flow under `key`, moves its values that
    /// other Flows hold to the shared values.
    pub(in crate::supervision::shard::state) fn keep_held(&mut self, now: u64, key: &Parts) {
        let owner = (key.0.tag().into_bytes(), key.1.clone());
        if !self.held.owners.contains_key(&owner) {
            return;
        }
        let refs: Vec<Vec<u8>> = self
            .held
            .holders
            .keys()
            .filter(|reference| self::owner(reference).as_ref() == Some(&owner))
            .cloned()
            .collect();
        for reference in refs {
            if self.shared.contains_key(&reference) {
                continue;
            }
            if let Some(bytes) = self.value_bytes(now, &reference) {
                let moved = Shared {
                    partition: key.0.clone(),
                    bytes,
                    expires_at: None,
                };
                self.shared.insert(reference, moved);
            }
        }
    }

    /// Every holding Flow and its references, for a shard image.
    pub(crate) fn holdings(&self) -> Vec<(&Key, &[Vec<u8>])> {
        let mut holdings: Vec<(&Key, &[Vec<u8>])> = self
            .held
            .by_flow
            .iter()
            .map(|(key, refs)| (key, refs.as_slice()))
            .collect();
        holdings.sort_unstable_by(|left, right| {
            (left.0.0.name(), &left.0.1).cmp(&(right.0.0.name(), &right.0.1))
        });
        holdings
    }

    /// Restores a Flow's holdings from a shard image.
    pub(crate) fn restore_holdings(&mut self, partition: Partition, id: Vec<u8>, refs: &[Vec<u8>]) {
        let key = super::super::flow_key(partition, id);
        self.hold(&key, refs);
    }
}
