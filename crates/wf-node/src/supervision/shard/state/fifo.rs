//! FIFO states (ADR-014 step 9b), as Elixir's `FifoLane`.
//!
//! A state whose policy mode is `fifo` runs one Flow at a time per
//! partition, oldest first. A lane is a type, a logical state (a running
//! Flow's is the state it was claimed from) and a partition; its Flows are
//! ordered by when they entered that state (`Entry::sequence`, stamped on
//! every change of logical state), running Flows first, as Elixir's lane
//! scores rank them: a Flow already running when its state turned FIFO
//! holds the lane (audit gap G77). A claim may take only a lane's head, and
//! only while the head waits and is due: a running head, or one not due yet,
//! blocks its lane. Lanes are kept only for types whose policy has a FIFO
//! state and rebuilt when such a policy is installed; when a head leaves its
//! lane the round reports the lane's queue as an arrival for parked claims.
//! Entering a FIFO state needs an explicit partition and priority 0, and a
//! Flow cannot enter one already running.

use std::collections::BTreeSet;

use super::Map;

use wf_application::commands::flow::policy::Mode;
use wf_application::commands::queue::{ClaimFlows, Partition, ServiceError};
use wf_domain::workflow::WorkflowRecord;

use super::super::super::waiters::Arrival;
use super::cold::Slot;
use super::{Entry, Key, Queue, RUNNING, ShardState};

/// Lanes by type and logical state, then partition: members as
/// [`member`] orders them.
pub(super) type Lanes = Map<(Vec<u8>, Vec<u8>), Map<Partition, BTreeSet<Member>>>;

/// A lane member: `0` for a running Flow, `1` for a waiting one, then its
/// entry sequence and key.
pub(super) type Member = (u8, u64, Key);

fn member(record: &WorkflowRecord, sequence: u64, key: &Key) -> Member {
    (u8::from(record.state() != RUNNING), sequence, key.clone())
}

/// How many Flows behind a lane's head a claim checks for an overdue
/// active-time limit (audit gap G75).
const FOLLOWER_SCAN: usize = 64;

fn terminal(state: &[u8]) -> bool {
    matches!(state, b"completed" | b"failed" | b"cancelled")
}

fn rejected(text: &str) -> ServiceError {
    ServiceError::rejected(text.to_owned())
}

/// The state a Flow is in for FIFO purposes: a running Flow's is the state
/// it was claimed from.
pub(super) fn logical_state(record: &WorkflowRecord) -> &[u8] {
    if record.state() == RUNNING {
        record.run_state().unwrap_or(b"queued")
    } else {
        record.state()
    }
}

impl ShardState {
    /// Whether `state` of `flow_type` is a FIFO state.
    pub(super) fn fifo(&self, flow_type: &[u8], state: &[u8]) -> bool {
        !self.policies.is_empty()
            && self
                .policy(flow_type)
                .is_some_and(|installed| installed.policy.mode(state) == Mode::Fifo)
    }

    /// Whether lanes are kept for `flow_type`.
    fn laned(&self, flow_type: &[u8]) -> bool {
        !self.policies.is_empty()
            && self.policy(flow_type).is_some_and(|installed| {
                installed
                    .policy
                    .states
                    .values()
                    .any(|state| state.mode == Some(Mode::Fifo))
            })
    }

    /// Elixir's `flow_require_fifo_entry/4` for a Flow about to be stored;
    /// `explicit` is whether the command named its partition, `None` when
    /// the command need not.
    pub(super) fn fifo_entry(
        &self,
        record: &WorkflowRecord,
        priority: u8,
        explicit: Option<bool>,
    ) -> Result<(), ServiceError> {
        self.fifo_guard(
            (record.workflow_type(), logical_state(record)),
            record.state() == RUNNING,
            priority,
            explicit,
        )
    }

    /// [`Self::fifo_entry`] before the record exists.
    pub(super) fn fifo_guard(
        &self,
        (flow_type, state): (&[u8], &[u8]),
        running: bool,
        priority: u8,
        explicit: Option<bool>,
    ) -> Result<(), ServiceError> {
        if !self.fifo(flow_type, state) {
            return Ok(());
        }
        if running {
            Err(rejected(
                "ERR flow direct running entry is not supported for fifo state",
            ))
        } else if explicit == Some(false) {
            Err(rejected(
                "ERR flow partition_key is required for fifo state",
            ))
        } else if priority != 0 {
            Err(rejected(
                "ERR flow priority is not supported for fifo state",
            ))
        } else {
            Ok(())
        }
    }

    /// Adds a stored Flow to its lane.
    pub(super) fn lane_insert(&mut self, key: &Key, entry: &Entry) {
        let record = &entry.record;
        if terminal(record.state()) || !self.laned(record.workflow_type()) {
            return;
        }
        self.lanes
            .entry((
                record.workflow_type().to_vec(),
                logical_state(record).to_vec(),
            ))
            .or_default()
            .entry(key.0.clone())
            .or_default()
            .insert(member(record, entry.sequence, key));
    }

    /// Removes a Flow from its lane; a removed head frees the lane.
    pub(super) fn lane_remove(&mut self, key: &Key, entry: &Entry) {
        let record = &entry.record;
        let lane = (
            record.workflow_type().to_vec(),
            logical_state(record).to_vec(),
        );
        let Some(partitions) = self.lanes.get_mut(&lane) else {
            return;
        };
        let Some(members) = partitions.get_mut(&key.0) else {
            return;
        };
        let member = member(record, entry.sequence, key);
        let head = members.first() == Some(&member);
        if !members.remove(&member) {
            return;
        }
        if members.is_empty() {
            partitions.remove(&key.0);
        } else if head {
            let arrival = Arrival {
                flow_type: lane.0.clone(),
                state: lane.1.clone(),
                priority: 0,
            };
            if !self.freed.contains(&arrival) {
                self.freed.push(arrival);
            }
        }
        if partitions.is_empty() {
            self.lanes.remove(&lane);
        }
    }

    /// Rebuilds `flow_type`'s lanes after its policy changed.
    pub(super) fn rebuild_lanes(&mut self, flow_type: &[u8]) {
        self.lanes
            .retain(|(laned, _), _| laned.as_slice() != flow_type);
        if !self.laned(flow_type) {
            return;
        }
        // Lanes hold live Flows, which are always in memory (ADR-016).
        let members: Vec<(Key, (Vec<u8>, Member))> = self
            .records
            .iter()
            .filter_map(|(key, slot)| match slot {
                Slot::Hot(entry) => Some((key, &**entry)),
                Slot::Warm(..) | Slot::Cold(_) => None,
            })
            .filter(|(_, entry)| {
                entry.record.workflow_type() == flow_type && !terminal(entry.record.state())
            })
            .map(|(key, entry)| {
                (
                    key.clone(),
                    (
                        logical_state(&entry.record).to_vec(),
                        member(&entry.record, entry.sequence, key),
                    ),
                )
            })
            .collect();
        for (key, (state, member)) in members {
            self.lanes
                .entry((flow_type.to_vec(), state))
                .or_default()
                .entry(key.0.clone())
                .or_default()
                .insert(member);
        }
    }

    /// Up to [`FOLLOWER_SCAN`] Flows behind each head of `queue`'s lanes
    /// the request may claim from, as Elixir's bounded follower scan.
    pub(super) fn lane_followers(
        &self,
        queue: &Queue,
        request: &ClaimFlows,
        partitions: Option<&[Partition]>,
    ) -> Vec<Key> {
        let lanes = self
            .lanes
            .get(&(queue.flow_type.clone(), queue.state.clone()));
        lanes
            .into_iter()
            .flatten()
            .filter(|(partition, _)| request.allows(partitions, partition))
            .flat_map(|(_, members)| members.iter().skip(1).take(FOLLOWER_SCAN))
            .map(|(_, _, key)| key.clone())
            .collect()
    }

    /// Queues of lanes freed since the last call, for parked claims.
    pub(crate) fn take_freed(&mut self) -> Vec<Arrival> {
        std::mem::take(&mut self.freed)
    }

    /// The heads of `queue`'s lanes a claim may take now, with their due
    /// slots; `None` when `queue` is not a FIFO state.
    pub(super) fn lane_heads<'a>(
        &'a self,
        queue: &Queue,
        now: u64,
        request: &ClaimFlows,
        partitions: Option<&[Partition]>,
    ) -> Option<Vec<((u64, u64), &'a Key)>> {
        if !self.fifo(&queue.flow_type, &queue.state) {
            return None;
        }
        let lanes = self
            .lanes
            .get(&(queue.flow_type.clone(), queue.state.clone()));
        let mut heads = Vec::new();
        for (partition, members) in lanes.into_iter().flatten() {
            if !request.allows(partitions, partition) {
                continue;
            }
            let Some((_, _, head)) = members.first() else {
                continue;
            };
            let Some((key, entry)) = self.hot_kv(head) else {
                continue;
            };
            let waiting = entry.record.state() == queue.state.as_slice()
                && entry.record.lease().is_none()
                && entry.priority == queue.priority;
            match entry.record.next_run_at() {
                Some(due) if waiting && due.get() <= now => {
                    heads.push(((due.get(), entry.sequence), key));
                }
                _ => {}
            }
        }
        Some(heads)
    }
}
