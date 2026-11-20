//! One shard thread (ADR-011).
//!
//! The thread owns its [`ShardState`]. It takes every queued request at once,
//! applies them in order with one committed time, appends the resulting
//! changes to the group log, and hands the replies to the completion thread,
//! which releases them after the sync. Replies of requests that changed
//! nothing wait for this shard's latest append too, because their answer may
//! depend on earlier changes that are not durable yet. After a failed append
//! the shard refuses everything until the node restarts and replays the log.

pub(crate) mod command;
mod handle;
pub(crate) mod image;
pub(crate) mod state;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use wf_application::commands::flow::cleanup::Cleaned;
use wf_application::commands::flow::start::RunSteps;
use wf_application::commands::flow::{FlowCommand, Term};
use wf_application::commands::queue::{
    ClaimFlows, ClaimedFlow, CompleteFlow, ItemOutcome, NewFlow, Partition, ServiceError,
};
use wf_storage::log::GroupLog;

use super::completer::{Completion, Finish};
use super::waiters::{Arrival, Waiters};
use command::Logged;
use handle::handle;
use state::{Peeked, ShardState};

/// Most requests one shard applies before appending.
const MAX_REQUESTS_PER_ROUND: usize = 256;
/// Most terminal records one round removes.
const MAX_REMOVALS_PER_ROUND: usize = 4_096;
/// How long an idle shard waits before it sweeps removals.
const IDLE_TICK: Duration = Duration::from_millis(250);

/// Called once with the request's result after it is durable.
pub(crate) type Done<T> = Box<dyn FnOnce(Result<T, ServiceError>) + Send>;

/// Work for one shard.
pub(crate) enum Request {
    /// Create Flows of one type and state; items keep their request position.
    Create {
        flow_type: Vec<u8>,
        state: Vec<u8>,
        run_at_ms: Option<u64>,
        priority: u8,
        items: Vec<(usize, NewFlow, Partition)>,
        done: Done<Vec<(usize, ItemOutcome)>>,
    },
    /// Claim up to `limit` due Flows from this shard.
    Claim {
        request: Arc<ClaimFlows>,
        limit: usize,
        partitions: Option<Arc<[Partition]>>,
        done: Done<Vec<ClaimedFlow>>,
    },
    /// Count what a claim could take now; answered at once, not after a sync.
    Peek {
        request: Arc<ClaimFlows>,
        limit: usize,
        partitions: Option<Arc<[Partition]>>,
        done: Box<dyn FnOnce(Peeked) + Send>,
    },
    /// Encode this shard's records for snapshot `snapshot` and log its
    /// marker; the image is handed over once the marker is durable.
    Capture { snapshot: u64, done: Done<Vec<u8>> },
    /// Complete Flows; items keep their request position.
    Complete {
        items: Vec<(usize, CompleteFlow, Partition)>,
        done: Done<Vec<(usize, ItemOutcome)>>,
    },
    /// A batch's mutations on this shard; all or nothing when `atomic`.
    FlowBatch {
        items: state::BatchItems,
        atomic: bool,
        done: Done<state::BatchReplies>,
    },
    /// Step chains for new Flows; outcomes are by request position.
    RunSteps {
        spec: Arc<RunSteps>,
        items: Vec<(usize, Vec<u8>, Partition)>,
        done: Done<Vec<(usize, Result<Term, ServiceError>)>>,
    },
    /// Values by reference; replies are by request position.
    ValueRead {
        refs: Vec<(usize, Vec<u8>)>,
        max_bytes: u64,
        done: Done<Vec<(usize, Term)>>,
    },
    /// `FLOW.RETENTION_CLEANUP` on this shard, up to `limit` Flows.
    Cleanup {
        now_ms: Option<u64>,
        limit: u64,
        done: Done<Cleaned>,
    },
    /// A policy command (ADR-014 step 9): a set or get on shard 0, or a
    /// copy of shard 0's durable set on another shard.
    Policy {
        command: state::PolicyCommand,
        done: Done<state::PolicyReply>,
    },
    /// Any other Flow command, for the Flow in `partition` (ADR-014).
    Flow {
        command: Box<FlowCommand>,
        partition: Partition,
        done: Done<Term>,
    },
}

/// The thread's connections to the rest of the node.
pub(crate) struct Links {
    /// This shard's index, named in its snapshot markers.
    pub(crate) index: u16,
    pub(crate) log: Arc<GroupLog>,
    pub(crate) completions: Sender<Completion>,
    pub(crate) ready: Arc<AtomicU64>,
    pub(crate) leased: Arc<AtomicU64>,
    pub(crate) waiters: Waiters,
    pub(crate) max_append_bytes: usize,
}

/// Committed time: the wall clock, never earlier than the previous round.
fn committed_now(previous: u64) -> u64 {
    let wall = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(previous);
    wall.max(previous)
}

/// The queues that gained Flows in a round, for parked claims.
fn arrivals(logged: &[Logged]) -> Vec<Arrival> {
    let mut arrivals: Vec<Arrival> = Vec::new();
    for change in logged {
        let arrival = match change {
            Logged::Create {
                flow_type,
                state,
                priority,
                ..
            } => Arrival {
                flow_type: flow_type.clone(),
                state: state.clone(),
                priority: *priority,
            },
            Logged::Put { image: bytes, .. } => match image::decode_one(bytes) {
                Some(restored) => Arrival {
                    flow_type: restored.record.workflow_type().to_vec(),
                    state: restored.record.state().to_vec(),
                    priority: restored.priority,
                },
                None => continue,
            },
            _ => continue,
        };
        if !arrivals.contains(&arrival) {
            arrivals.push(arrival);
        }
    }
    arrivals
}

/// Appends the round's changes in chunks that fit one log block; returns the
/// sequence covering all of them, or `None` when an append failed.
fn append(log: &GroupLog, logged: &[Logged], max_bytes: usize, previous: u64) -> Option<u64> {
    let mut sequence = previous;
    let mut chunk: Vec<Vec<u8>> = Vec::new();
    let mut chunk_bytes = 0_usize;
    for change in logged {
        let record = change.encode()?;
        let size = record.len().saturating_add(4);
        if !chunk.is_empty() && chunk_bytes.saturating_add(size) > max_bytes {
            sequence = log.append_batch(std::mem::take(&mut chunk)).ok()?;
            chunk_bytes = 0;
        }
        chunk_bytes = chunk_bytes.saturating_add(size);
        chunk.push(record);
    }
    if !chunk.is_empty() {
        sequence = log.append_batch(chunk).ok()?;
    }
    Some(sequence)
}

/// The shard thread's loop; returns when every sender is gone.
pub(crate) fn run(mut state: ShardState, requests: &Receiver<Request>, links: &Links) {
    let mut now = 0;
    let mut sequence = 0;
    let mut failed = false;
    loop {
        let mut round = match requests.recv_timeout(IDLE_TICK) {
            Ok(first) => vec![first],
            Err(RecvTimeoutError::Timeout) => Vec::new(),
            Err(RecvTimeoutError::Disconnected) => break,
        };
        while !round.is_empty() && round.len() < MAX_REQUESTS_PER_ROUND {
            match requests.try_recv() {
                Ok(request) => round.push(request),
                Err(_) => break,
            }
        }
        now = committed_now(now);
        let mut logged = Vec::new();
        if !failed {
            logged.extend(state.sweep_timeouts(now, MAX_REMOVALS_PER_ROUND));
            logged.extend(state.sweep(now, MAX_REMOVALS_PER_ROUND));
        }
        if round.is_empty() && logged.is_empty() {
            continue;
        }
        let finishes: Vec<Finish> = round
            .into_iter()
            .filter_map(|request| handle(&mut state, (links.index, now), request, &mut logged))
            .collect();
        // Parents of children that finished this round, on this shard.
        logged.extend(state.settle_children(now));
        if !failed {
            match append(&links.log, &logged, links.max_append_bytes, sequence) {
                Some(covered) => sequence = covered,
                None => failed = true,
            }
        }
        links.ready.store(state.ready(), Ordering::Relaxed);
        links.leased.store(state.leased(), Ordering::Relaxed);
        // A FIFO lane whose head left it has a new head to claim.
        let freed = state.take_freed();
        if !freed.is_empty()
            || logged
                .iter()
                .any(|change| matches!(change, Logged::Create { .. } | Logged::Put { .. }))
        {
            links.waiters.arrived(|| {
                let mut arrived = arrivals(&logged);
                arrived.extend(freed);
                arrived
            });
        }
        if failed {
            for finish in finishes {
                finish(Err(ServiceError::Unavailable));
            }
        } else if !finishes.is_empty()
            && let Err(returned) = links.completions.send(Completion { sequence, finishes })
        {
            for finish in returned.0.finishes {
                finish(Err(ServiceError::Unavailable));
            }
        }
    }
}
