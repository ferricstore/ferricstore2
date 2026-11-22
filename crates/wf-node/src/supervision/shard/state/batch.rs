//! Batches on one shard (audit gaps G26-G28). A batch is decided in units:
//! each item alone, the items of one partition together, or every item
//! together. A unit with a failing item changes nothing: its Flows are put
//! back, the child events and credit releases it queued for the round are
//! dropped, and each of its items answers the failure. (Closing parents and
//! freed lanes it queued stay: both are checked again when acted on, and
//! find the Flows as they were.)

use wf_application::commands::flow::Term;
use wf_application::commands::flow::mutate::Mutation;
use wf_application::commands::flow::record::FlowMeta;
use wf_application::commands::queue::{Partition, ServiceError};
use wf_domain::workflow::WorkflowRecord;

use super::{Key, Logged, Map, ShardState, flow_key};

/// A batch's items on this shard: request position, mutation, partition.
pub(crate) type BatchItems = Vec<(usize, Mutation, Partition)>;
/// Per-item replies by request position.
pub(crate) type BatchReplies = Vec<(usize, Result<Term, ServiceError>)>;

/// The all-or-none unit of a batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    /// Each item alone: an independent batch.
    Item,
    /// The items of one partition together: a batch whose items name
    /// their own partitions.
    Partition,
    /// Every item together: a batch in one named partition.
    Batch,
}

impl Unit {
    /// The unit of a batch: each item when `independent`, every item when
    /// it names one partition (`whole`), else each partition.
    pub(crate) const fn of(independent: bool, whole: bool) -> Self {
        match (independent, whole) {
            (true, _) => Self::Item,
            (false, true) => Self::Batch,
            (false, false) => Self::Partition,
        }
    }
}

/// `items` grouped to be decided: one group per partition for
/// `Unit::Partition`, keeping request order within each and the groups in
/// the order of their first item; else one group. `item` reads an item's
/// partition.
pub(super) fn units<T>(items: Vec<T>, unit: Unit, item: fn(&T) -> &Partition) -> Vec<Vec<T>> {
    match unit {
        Unit::Item | Unit::Batch => vec![items],
        Unit::Partition => {
            let mut index: Map<Partition, usize> = Map::default();
            let mut units: Vec<Vec<T>> = Vec::new();
            for one in items {
                let at = *index
                    .entry(item(&one).clone())
                    .or_insert_with(|| units.len());
                match units.get_mut(at) {
                    Some(unit) => unit.push(one),
                    None => units.push(vec![one]),
                }
            }
            units
        }
    }
}

/// How many child events and credit releases the round held when a unit
/// began.
#[derive(Clone, Copy)]
struct Marks(usize, usize);

impl ShardState {
    fn marks(&self) -> Marks {
        Marks(self.child_events.len(), self.credit_releases.len())
    }

    /// Drops the child events and credit releases queued since `marks`.
    fn drop_since(&mut self, Marks(events, credits): Marks) {
        self.child_events.truncate(events);
        self.credit_releases.truncate(credits);
    }

    /// Decides a batch's items on this shard in its units.
    pub(crate) fn mutate_batch(
        &mut self,
        wall: u64,
        items: BatchItems,
        unit: Unit,
    ) -> (BatchReplies, Vec<Logged>) {
        let mut replies = Vec::with_capacity(items.len());
        let mut logged = Vec::new();
        let atomic = unit != Unit::Item;
        for group in units(items, unit, |(_, _, partition)| partition) {
            let (decided, changes) = self.mutate_unit(wall, group, atomic);
            replies.extend(decided);
            logged.extend(changes);
        }
        (replies, logged)
    }

    /// Decides one unit; `atomic` puts it back when an item fails.
    fn mutate_unit(
        &mut self,
        wall: u64,
        items: BatchItems,
        atomic: bool,
    ) -> (BatchReplies, Vec<Logged>) {
        let positions: Vec<usize> = if atomic {
            items.iter().map(|(position, _, _)| *position).collect()
        } else {
            Vec::new()
        };
        let marks = self.marks();
        let mut replies = Vec::with_capacity(items.len());
        let mut logged = Vec::new();
        let mut saved: Vec<(Key, Option<Saved>)> = Vec::new();
        for (position, mutation, partition) in items {
            let key: Key = flow_key(partition.clone(), mutation.id.clone());
            // A finished Flow on disk is loaded first, so the save holds it
            // (ADR-016); one that cannot be read fails like a refusal.
            let loaded = self.warm(&key);
            if atomic && loaded.is_ok() {
                saved.push((key.clone(), self.save(&key)));
            }
            match loaded.and_then(|()| self.try_mutate(wall, &mutation, partition)) {
                Ok((reply, change)) => {
                    logged.extend(change);
                    replies.push((position, Ok(reply)));
                }
                Err(error) if atomic => {
                    self.drop_since(marks);
                    self.roll_back(saved);
                    let failed = positions
                        .into_iter()
                        .map(|position| (position, Err(error.clone())))
                        .collect();
                    return (failed, Vec::new());
                }
                Err(error) => replies.push((position, Err(error))),
            }
        }
        (replies, logged)
    }

    /// Puts back the Flows an atomic unit changed, latest first.
    fn roll_back(&mut self, saved: Vec<(Key, Option<Saved>)>) {
        for (key, entry) in saved.into_iter().rev() {
            self.restore_saved(key, entry);
        }
    }

    fn save(&self, key: &Key) -> Option<Saved> {
        self.hot(key).map(|entry| Saved {
            record: entry.record.clone(),
            priority: entry.priority,
            meta: entry.meta.clone(),
            history: entry.history.clone(),
        })
    }

    fn restore_saved(&mut self, key: Key, saved: Option<Saved>) {
        match saved {
            Some(saved) => {
                self.reinstate(
                    key,
                    (saved.record, saved.priority, saved.meta, saved.history),
                );
            }
            None => {
                self.take_entry(&key);
            }
        }
    }
}

/// A Flow as it was before an atomic unit touched it.
struct Saved {
    record: WorkflowRecord,
    priority: u8,
    meta: Option<Box<FlowMeta>>,
    history: super::events::History,
}
