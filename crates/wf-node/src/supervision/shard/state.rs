//! One shard's records and claim index (ADR-011).
//!
//! Every change goes through the domain reducer. [`ShardState::apply`] is
//! the only mutation path, used both live and when replaying the log, so the
//! rebuilt state matches the state that was acknowledged. Claim selection
//! uses this shard's own due index; the reducer then decides each named
//! claim. Records that are scheduled, unleased and not terminal are in the
//! due index by due time; leased running records are in it under state
//! `running` by lease deadline, for reclaim (ADR-013). Terminal records are
//! kept until their retention ends, then removed by a logged sweep.

mod capture;
mod claim;
mod flow;
mod mutate;
mod retention;

pub(crate) use claim::Peeked;
pub(crate) use mutate::{BatchItems, BatchReplies};

use std::collections::{BTreeMap, HashMap};

use wf_application::commands::flow::record::FlowMeta;
use wf_application::commands::queue::{
    CompleteFlow, ItemOutcome, MAX_FLOW_ID_BYTES, NewFlow, Partition, ServiceError,
};
use wf_domain::claim::Claim;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::workflow::{
    self, CommittedCommand, Create, Operation, Outcome, Terminal, WorkflowRecord, WorkflowStatus,
};
use wf_types::identity::RecordId;
use wf_types::version::{Fence, TimestampMillis};

use super::command::Logged;
use super::image;

type Key = (Partition, Vec<u8>);

/// The physical state of leased records.
const RUNNING: &[u8] = b"running";

struct Entry {
    record: WorkflowRecord,
    priority: u8,
    sequence: u64,
    /// Fields beyond the lifecycle record; `None` for a Flow made by the
    /// queue fast path, which has the defaults (ADR-014).
    meta: Option<Box<FlowMeta>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Queue {
    flow_type: Vec<u8>,
    state: Vec<u8>,
    priority: u8,
}

/// Records of one shard and their due index.
pub(crate) struct ShardState {
    records: HashMap<Key, Entry>,
    /// Due time and creation sequence to record key, per queue.
    queues: HashMap<Queue, BTreeMap<(u64, u64), Key>>,
    /// Removal time and creation sequence to key, for terminal records.
    expiries: BTreeMap<(u64, u64), Key>,
    retention_ms: u64,
    next_sequence: u64,
    ready: u64,
    leased: u64,
}

fn valid_id(id: &[u8]) -> Result<(), ServiceError> {
    if id.is_empty() || id.len() > MAX_FLOW_ID_BYTES {
        Err(ServiceError::InvalidId)
    } else {
        Ok(())
    }
}

fn token(bytes: &[u8]) -> Result<LeaseToken, ServiceError> {
    LeaseToken::new(bytes.to_vec()).map_err(ServiceError::Domain)
}

impl ShardState {
    /// An empty shard that keeps terminal records for `retention_ms`.
    pub(crate) fn new(retention_ms: u64) -> Self {
        Self {
            records: HashMap::new(),
            queues: HashMap::new(),
            expiries: BTreeMap::new(),
            retention_ms,
            next_sequence: 0,
            ready: 0,
            leased: 0,
        }
    }

    /// Records in the due index, claimable now or later.
    pub(crate) const fn ready(&self) -> u64 {
        self.ready
    }

    /// Leased running records, reclaimable once their lease expires.
    pub(crate) const fn leased(&self) -> u64 {
        self.leased
    }

    /// Applies one change through the reducer and stores its write.
    pub(crate) fn apply(&mut self, logged: &Logged) -> Result<Outcome, ServiceError> {
        let (now, partition, id, operation, priority) = match logged {
            Logged::Expire { keys, .. } => return Ok(self.expire(keys)),
            Logged::Put { image: bytes, .. } => {
                let restored = image::decode_one(bytes).ok_or(ServiceError::Unavailable)?;
                self.put(
                    (restored.partition, restored.id),
                    restored.record,
                    restored.priority,
                    restored.meta.map(Box::new),
                );
                return Ok(Outcome::Changed);
            }
            // A marker changes no record; replay handles it before apply.
            Logged::Marker { .. } => return Ok(Outcome::NotEligible),
            Logged::Create {
                now,
                partition,
                id,
                flow_type,
                state,
                run_at,
                payload,
                priority,
            } => {
                let create = Create {
                    workflow_type: flow_type.clone(),
                    state: state.clone(),
                    run_at: run_at.map(TimestampMillis::new),
                    payload: payload.clone(),
                };
                (
                    now,
                    partition,
                    id,
                    Operation::Create(create),
                    Some(*priority),
                )
            }
            Logged::Claim {
                now,
                partition,
                id,
                expected_state,
                worker,
                token: lease,
                lease_ms,
                reclaim,
            } => {
                let claim = Claim {
                    expected_state: expected_state.clone(),
                    worker: worker.clone(),
                    token: token(lease)?,
                    lease_ms: *lease_ms,
                };
                let operation = if *reclaim {
                    Operation::Reclaim(claim)
                } else {
                    Operation::Claim(claim)
                };
                (now, partition, id, operation, None)
            }
            Logged::Complete {
                now,
                partition,
                id,
                token: lease,
                fence,
                result,
            } => {
                let terminal = Terminal {
                    guard: LeaseGuard {
                        token: token(lease)?,
                        fence: Fence::new(*fence),
                    },
                    payload: None,
                    value: result.clone(),
                };
                (now, partition, id, Operation::Complete(terminal), None)
            }
        };
        valid_id(id)?;
        let command = CommittedCommand {
            id: RecordId::new(id.clone()).map_err(|_| ServiceError::InvalidId)?,
            now: TimestampMillis::new(*now),
            operation,
        };
        let key = (partition.clone(), id.clone());
        let current = self.records.get(&key).map(|entry| &entry.record);
        let (outcome, write, _history) = workflow::reduce(current, &command)
            .map_err(ServiceError::Domain)?
            .into_parts();
        if let Some(write) = write {
            self.store(key, write.record, priority);
        }
        Ok(outcome)
    }

    /// Stores a reducer write, keeping the record's priority and fields.
    fn store(&mut self, key: Key, record: WorkflowRecord, new_priority: Option<u8>) {
        let (priority, sequence, meta) = match self.take_entry(&key) {
            Some(previous) => (previous.priority, previous.sequence, previous.meta),
            None => (new_priority.unwrap_or(0), self.sequence(), None),
        };
        self.insert(
            key,
            Entry {
                record,
                priority,
                sequence,
                meta,
            },
        );
    }

    /// Stores a whole Flow, replacing any record under its key.
    pub(super) fn put(
        &mut self,
        key: Key,
        record: WorkflowRecord,
        priority: u8,
        meta: Option<Box<FlowMeta>>,
    ) {
        let sequence = match self.take_entry(&key) {
            Some(previous) => previous.sequence,
            None => self.sequence(),
        };
        self.insert(
            key,
            Entry {
                record,
                priority,
                sequence,
                meta,
            },
        );
    }

    fn sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        sequence
    }

    /// Removes a record and its index entries.
    fn take_entry(&mut self, key: &Key) -> Option<Entry> {
        let previous = self.records.remove(key)?;
        self.unindex(key, &previous);
        self.unschedule_removal(&previous);
        Some(previous)
    }

    fn insert(&mut self, key: Key, entry: Entry) {
        self.index(&key, &entry);
        self.schedule_removal(&key, &entry);
        self.records.insert(key, entry);
    }

    /// The index entry of a record: due time for a scheduled unleased record,
    /// lease deadline for a leased running one.
    fn queue_slot(entry: &Entry) -> Option<(Queue, (u64, u64))> {
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

    fn index(&mut self, key: &Key, entry: &Entry) {
        if let Some((queue, slot)) = Self::queue_slot(entry) {
            self.count(&queue, true);
            self.queues
                .entry(queue)
                .or_default()
                .insert(slot, key.clone());
        }
    }

    fn unindex(&mut self, key: &Key, entry: &Entry) {
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

    /// Creates one Flow; returns its outcome and the change to log.
    pub(crate) fn create(
        &mut self,
        now: u64,
        shape: (&[u8], &[u8], Option<u64>, u8),
        flow: NewFlow,
        partition: Partition,
    ) -> (ItemOutcome, Option<Logged>) {
        let (flow_type, state, run_at, priority) = shape;
        let logged = Logged::Create {
            now,
            partition,
            id: flow.id,
            flow_type: flow_type.to_vec(),
            state: state.to_vec(),
            run_at,
            payload: flow.payload,
            priority,
        };
        settle(self.apply(&logged), logged)
    }

    /// Completes one claimed Flow; returns its outcome and the change to log.
    pub(crate) fn complete(
        &mut self,
        now: u64,
        completion: CompleteFlow,
        partition: Partition,
    ) -> (ItemOutcome, Option<Logged>) {
        let logged = Logged::Complete {
            now,
            partition,
            id: completion.id,
            token: completion.lease_token,
            fence: completion.fence,
            result: completion.result,
        };
        settle(self.apply(&logged), logged)
    }
}

/// An item outcome, and the change to log when the reducer changed the record.
fn settle(applied: Result<Outcome, ServiceError>, logged: Logged) -> (ItemOutcome, Option<Logged>) {
    match applied {
        Ok(Outcome::Changed) => (Ok(()), Some(logged)),
        Ok(_) => (Ok(()), None),
        Err(error) => (Err(error), None),
    }
}
