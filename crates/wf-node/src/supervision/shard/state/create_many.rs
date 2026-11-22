//! `FLOW.CREATE_MANY` beyond the queue fast path on one shard (audit gap
//! G2): each item is created as `FLOW.CREATE` would; in an all-or-nothing
//! unit (G27), a failing item takes back every Flow the unit created and
//! fails every item of the unit with its error.

use wf_application::commands::flow::Term;
use wf_application::commands::flow::create::CreateFlow;
use wf_application::commands::queue::{Partition, ServiceError};

use super::super::command::Logged;
use super::batch::{Unit, units};
use super::{BatchReplies, Key, ShardState, flow_key};

/// One shard's items of a batch, with their request positions.
pub(crate) type CreateItems = Vec<(usize, CreateFlow, Partition)>;

/// One item's reply, and the Flow it created with the change to log.
type Created = (Result<Term, ServiceError>, Option<(Key, Logged)>);

impl ShardState {
    /// Creates one shard's items, decided in `unit`s.
    pub(crate) fn create_batch(
        &mut self,
        wall: u64,
        items: CreateItems,
        unit: Unit,
    ) -> (BatchReplies, Vec<Logged>) {
        let mut replies = Vec::with_capacity(items.len());
        let mut logged = Vec::new();
        let atomic = unit != Unit::Item;
        for group in units(items, unit, |(_, _, partition)| partition) {
            let (decided, changes) = self.create_unit(wall, group, atomic);
            replies.extend(decided);
            logged.extend(changes);
        }
        (replies, logged)
    }

    /// Creates one unit; `atomic` takes back what it created when an item
    /// fails.
    fn create_unit(
        &mut self,
        wall: u64,
        items: CreateItems,
        atomic: bool,
    ) -> (BatchReplies, Vec<Logged>) {
        let positions: Vec<usize> = items.iter().map(|(position, _, _)| *position).collect();
        let mut replies = Vec::with_capacity(items.len());
        let mut logged = Vec::new();
        let mut created = Vec::new();
        for (position, create, partition) in items {
            let (outcome, change) = self.create_item(wall, create, partition);
            if let Some((key, change)) = change {
                created.push(key);
                logged.push(change);
            }
            let Err(error) = outcome else {
                replies.push((position, outcome));
                continue;
            };
            if !atomic {
                replies.push((position, Err(error)));
                continue;
            }
            for key in created.iter().rev() {
                drop(self.take_entry(key));
            }
            let failed = positions
                .into_iter()
                .map(|position| (position, Err(error.clone())))
                .collect();
            return (failed, Vec::new());
        }
        (replies, logged)
    }

    fn create_item(&mut self, wall: u64, create: CreateFlow, partition: Partition) -> Created {
        let key = flow_key(partition.clone(), create.id.clone());
        // An existing finished Flow is read to compare (ADR-016).
        if let Err(error) = self.warm(&key) {
            return (Err(error), None);
        }
        let now = create.now_ms.unwrap_or(wall);
        let (outcome, change) = self.create_flow(now, create, partition);
        (outcome, change.map(|change| (key, change)))
    }
}
