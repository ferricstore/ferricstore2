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
mod catalog;
mod children;
mod claim;
pub(crate) mod credit;
mod effect;
pub(crate) mod events;
mod fifo;
mod flow;
mod governance;
mod index;
mod mutate;
mod policy;
mod query;
mod retention;
mod rewind;
pub(crate) mod schedule;
mod signal;
mod steps;
mod store;
mod timeout;
mod value;

pub(crate) use claim::Peeked;
pub(crate) use mutate::{BatchItems, BatchReplies};
pub(crate) use policy::{PolicyCommand, PolicyReply};
pub(crate) use schedule::{ScheduleOp, ScheduleReply};

use std::collections::{BTreeMap, HashMap};

/// A hash map on the shard's hot paths: foldhash, randomly seeded per map,
/// several times cheaper than `SipHash` on short keys (ADR-015).
pub(super) type Map<K, V> = HashMap<K, V, foldhash::fast::RandomState>;
use std::sync::Arc;

use events::History;
use wf_application::commands::flow::governance::Kind;
use wf_application::commands::flow::policy::Installed;
use wf_application::commands::flow::record::FlowMeta;
use wf_application::commands::queue::{
    CompleteFlow, ItemOutcome, MAX_FLOW_ID_BYTES, NewFlow, Partition, ServiceError,
};
use wf_domain::claim::Claim;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::workflow::{
    self, CommittedCommand, Create, Operation, Outcome, Terminal, WorkflowRecord,
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
    /// One event per record version, oldest first (ADR-014).
    history: History,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Queue {
    flow_type: Vec<u8>,
    state: Vec<u8>,
    priority: u8,
}

/// Records of one shard and their due index.
pub(crate) struct ShardState {
    /// Boxed, so the table moves pointers, not whole Flows, when it grows
    /// or a change takes a Flow out and puts it back (ADR-015).
    records: Map<Key, Box<Entry>>,
    /// Due time and creation sequence to record key, per queue.
    queues: Map<Queue, BTreeMap<(u64, u64), Key>>,
    /// Removal time and creation sequence to key, for terminal records.
    expiries: BTreeMap<(u64, u64), Key>,
    retention_ms: u64,
    next_sequence: u64,
    ready: u64,
    leased: u64,
    /// Shared values by reference (ADR-014 step 5).
    shared: Map<Vec<u8>, value::Shared>,
    /// Governance records by kind and name, in stored form (ADR-014 step
    /// 15).
    governance: BTreeMap<(Kind, Vec<u8>), Vec<u8>>,
    /// Credits of Flows that left `running`, to release (ADR-014 step 15).
    credit_releases: Vec<wf_application::commands::flow::effect::Credit>,
    /// Named partitions by hash tag, to find a Flow from a value reference;
    /// rebuilt on a miss when Flows were added since (`tags_at`).
    tags: Map<String, Partition>,
    tags_at: u64,
    /// Whether changes are live; replay re-applies logged parent changes
    /// instead of queueing child events.
    live: bool,
    /// Child events waiting for this round to settle them.
    child_events: Vec<children::ChildEvent>,
    /// Flow type policies by type (ADR-014 step 9).
    policies: Map<Vec<u8>, Arc<Installed>>,
    /// FIFO lanes, and the queues of lanes freed this round (step 9b).
    lanes: fifo::Lanes,
    freed: Vec<super::super::waiters::Arrival>,
    /// Deadlines of live Flows with an active-time limit, and when the
    /// periodic timeout sweep runs next (0 until the first round).
    deadlines: BTreeMap<(u64, u64), Key>,
    timeouts_due: u64,
    /// Every Flow by type, state and partition (ADR-014 step 11).
    catalog: catalog::Catalog,
    /// The key of the query cursors this shard issues (step 12).
    cursor_key: [u8; 32],
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
            records: Map::default(),
            queues: Map::default(),
            expiries: BTreeMap::new(),
            retention_ms,
            next_sequence: 0,
            ready: 0,
            leased: 0,
            shared: Map::default(),
            governance: BTreeMap::new(),
            credit_releases: Vec::new(),
            tags: Map::default(),
            tags_at: 0,
            live: false,
            child_events: Vec::new(),
            policies: Map::default(),
            lanes: Map::default(),
            freed: Vec::new(),
            deadlines: BTreeMap::new(),
            timeouts_due: 0,
            catalog: Map::default(),
            cursor_key: query::cursor_key(),
        }
    }

    /// Starts live operation once replay is done.
    pub(crate) fn go_live(&mut self) {
        self.live = true;
    }

    /// Records in the due index, claimable now or later.
    pub(crate) const fn ready(&self) -> u64 {
        self.ready
    }

    /// Leased running records, reclaimable once their lease expires.
    pub(crate) const fn leased(&self) -> u64 {
        self.leased
    }

    /// Applies a change that stores state as logged instead of deciding it:
    /// a sweep, a marker, a shared value or an after-image. `None` for a
    /// change the reducer decides.
    fn apply_stored(&mut self, logged: &Logged) -> Option<Result<Outcome, ServiceError>> {
        Some(match logged {
            Logged::Expire { keys, .. } => Ok(self.expire(keys)),
            Logged::Shared {
                partition,
                reference,
                bytes,
                expires_at,
            } => {
                self.store_shared(
                    partition.clone(),
                    reference.clone(),
                    bytes.clone(),
                    *expires_at,
                );
                Ok(Outcome::Changed)
            }
            Logged::Put { image: bytes, .. } => match image::decode_one(bytes) {
                Some(restored) => {
                    // An after-image carries the events its change appended.
                    self.put_events(
                        (restored.partition, restored.id),
                        (
                            restored.record,
                            restored.priority,
                            restored.meta.map(Box::new),
                        ),
                        &restored.history.unwrap_or_default(),
                    );
                    Ok(Outcome::Changed)
                }
                None => Err(ServiceError::Unavailable),
            },
            Logged::Policy {
                flow_type,
                generation,
                policy,
            } => self
                .restore_policy(flow_type, *generation, policy)
                .map(|()| Outcome::Changed),
            Logged::Governance {
                kind, name, value, ..
            } => self.restore_governance(*kind, name, value.clone()),
            // A marker changes no record; replay handles it before apply.
            Logged::Marker { .. } => Ok(Outcome::NotEligible),
            Logged::Group(changes) => changes
                .iter()
                .map(|change| self.apply(change))
                .find(Result::is_err)
                .unwrap_or(Ok(Outcome::Changed)),
            Logged::Create { .. } | Logged::Claim { .. } | Logged::Complete { .. } => return None,
        })
    }

    /// Applies one change through the reducer and stores its write.
    pub(crate) fn apply(&mut self, logged: &Logged) -> Result<Outcome, ServiceError> {
        if let Some(applied) = self.apply_stored(logged) {
            return applied;
        }
        let (now, partition, id, operation, priority) = match logged {
            Logged::Expire { .. }
            | Logged::Shared { .. }
            | Logged::Put { .. }
            | Logged::Group(_)
            | Logged::Policy { .. }
            | Logged::Governance { .. }
            | Logged::Marker { .. } => return Ok(Outcome::NotEligible),
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

    /// Creates one Flow; returns its outcome and the change to log.
    pub(crate) fn create(
        &mut self,
        now: u64,
        shape: (&[u8], &[u8], Option<u64>, u8),
        flow: NewFlow,
        partition: Partition,
    ) -> (ItemOutcome, Option<Logged>) {
        let (flow_type, state, run_at, priority) = shape;
        let explicit = matches!(partition, Partition::Named(_));
        if let Err(error) = self.fifo_guard((flow_type, state), false, priority, Some(explicit)) {
            return (Err(error), None);
        }
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
