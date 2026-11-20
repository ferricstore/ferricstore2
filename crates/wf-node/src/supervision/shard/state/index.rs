//! The claim index: due Flows by queue, leased ones by lease deadline, and
//! the FIFO lanes beside them (ADR-011, ADR-013, ADR-014 step 9b).

use std::collections::BTreeMap;

use wf_domain::workflow::WorkflowStatus;

use super::{Entry, Key, Queue, RUNNING, ShardState};

impl ShardState {
    /// The index entry of a record: due time for a scheduled unleased record,
    /// lease deadline for a leased running one.
    pub(super) fn queue_slot(entry: &Entry) -> Option<(Queue, (u64, u64))> {
        let record = &entry.record;
        let due = match (record.status(), record.lease()) {
            (WorkflowStatus::Scheduled, None) => record.next_run_at()?,
            (_, Some(lease)) if record.state() == RUNNING => lease.deadline,
            _ => return None,
        };
        let queue = Queue {
            flow_type: record.workflow_type().to_vec(),
            state: record.state().to_vec(),
            priority: entry.priority,
        };
        Some((queue, (due.get(), entry.sequence)))
    }

    fn count(&mut self, queue: &Queue, added: bool) {
        let counter = if queue.state == RUNNING {
            &mut self.leased
        } else {
            &mut self.ready
        };
        *counter = if added {
            counter.saturating_add(1)
        } else {
            counter.saturating_sub(1)
        };
    }

    pub(super) fn index(&mut self, key: &Key, entry: &Entry) {
        self.lane_insert(key, entry);
        if let Some((queue, slot)) = Self::queue_slot(entry) {
            self.count(&queue, true);
            self.queues
                .entry(queue)
                .or_default()
                .insert(slot, key.clone());
        }
    }

    pub(super) fn unindex(&mut self, key: &Key, entry: &Entry) {
        self.lane_remove(key, entry);
        let Some((queue, slot)) = Self::queue_slot(entry) else {
            return;
        };
        let removed = self
            .queues
            .get_mut(&queue)
            .is_some_and(|due| due.remove(&slot).is_some_and(|indexed| indexed == *key));
        if removed {
            self.count(&queue, false);
            if self.queues.get(&queue).is_some_and(BTreeMap::is_empty) {
                self.queues.remove(&queue);
            }
        }
    }
}
