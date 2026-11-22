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
mod request;
pub(crate) mod role;
pub(crate) mod sink;
pub(crate) mod state;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use wf_application::commands::flow::cleanup::Cleaned;
use wf_application::commands::flow::effect::Credit;
use wf_application::commands::flow::governance::{Command as GovernanceCommand, Listed};
use wf_application::commands::flow::info::{Read, Tally};
use wf_application::commands::flow::policy::Installed;
use wf_application::commands::flow::start::RunSteps;
use wf_application::commands::flow::{FlowCommand, Term};
use wf_application::commands::queue::{
    ClaimFlows, ClaimedFlow, CompleteFlow, ItemOutcome, NewFlow, Partition, ServiceError,
};

use super::completer::Finish;
use super::waiters::{Arrival, Waiters};
use command::Logged;
use handle::handle;
use role::Role;
use sink::{Pending, Sink};
use state::credit::CreditPool;
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
        /// A governed claim's credits, one per Flow taken (ADR-014 step 15).
        credits: Option<Arc<CreditPool>>,
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
    /// A batch's creates on this shard, decided in `unit`s (audit gaps G2
    /// and G27).
    CreateBatch {
        items: state::CreateItems,
        unit: state::Unit,
        done: Done<state::BatchReplies>,
    },
    /// A batch's mutations on this shard, decided in `unit`s (G26-G28).
    FlowBatch {
        items: state::BatchItems,
        unit: state::Unit,
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
    /// A schedule operation (ADR-014 step 13).
    Schedule {
        op: state::ScheduleOp,
        done: Done<state::ScheduleReply>,
    },
    /// This shard's part of a count or discovery read (ADR-014 step 11).
    Read { read: Arc<Read>, done: Done<Tally> },
    /// This shard's part of a governance listing (ADR-014 step 15).
    GovernanceList {
        command: Arc<GovernanceCommand>,
        done: Done<Listed>,
    },
    /// Any other Flow command, for the Flow in `partition` (ADR-014).
    Flow {
        command: Box<FlowCommand>,
        partition: Partition,
        done: Done<Term>,
    },
    /// A follower applies the leader's committed changes (ADR-018).
    Replay {
        changes: Vec<Logged>,
        done: Done<()>,
    },
    /// The shard serves as the leader's, with shard 0's policies.
    Live {
        policies: Vec<Arc<Installed>>,
        done: Done<()>,
    },
    /// The shard follows again.
    Quiet { done: Done<()> },
    /// The policies this shard holds.
    Policies { done: Done<Vec<Arc<Installed>>> },
    /// This shard as a portable image, for a group snapshot (phase 3).
    Export { done: Done<Vec<u8>> },
    /// Replace this shard with an installed snapshot's image.
    Install { image: Vec<u8>, done: Done<()> },
}

/// The thread's connections to the rest of the node.
pub(crate) struct Links {
    /// This shard's index, named in its snapshot markers.
    pub(crate) index: u16,
    /// Where the shard's rounds go (ADR-018).
    pub(crate) sink: Sink,
    /// Whether the shard decides client requests (ADR-018).
    pub(crate) role: Role,
    pub(crate) ready: Arc<AtomicU64>,
    pub(crate) leased: Arc<AtomicU64>,
    /// The earliest lease deadline, `u64::MAX` with none (audit gap G24).
    pub(crate) expiry: Arc<AtomicU64>,
    /// Due-indexed Flows by claim priority (audit gap G25).
    pub(crate) by_priority: Arc<[AtomicU64; 3]>,
    pub(crate) waiters: Waiters,
    /// Delivers the credits of Flows that left `running` (ADR-014 step 15).
    pub(crate) release: Releases,
    /// Wakes the queued schedules of targets that finished (audit G47).
    pub(crate) wake: Wakes,
    /// Renews a governed Flow's credit lease before its lease is extended
    /// (audit G54).
    pub(crate) renew: RenewFirst,
    /// Items sent to this shard and not yet taken (ADR-017).
    pub(crate) backlog: Arc<AtomicUsize>,
}

/// Delivers limit credits for release.
pub(crate) type Releases = Arc<dyn Fn(Vec<Credit>) + Send + Sync>;

/// Renews a credit lease, then sends the extension it holds back to its
/// Flow's shard, or answers the extension with the renewal's refusal.
pub(crate) type RenewFirst =
    Arc<dyn Fn(state::credit::Renewal, Box<FlowCommand>, Partition, Done<Term>) + Send + Sync>;

/// Delivers finished targets' schedules to wake.
pub(crate) type Wakes = Arc<dyn Fn(Vec<state::schedule::Wake>) + Send + Sync>;

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

/// Releases `credits` once the round that freed them is durable.
fn release_once_durable(release: Releases, credits: Vec<Credit>) -> Finish {
    Box::new(move |durable: Result<(), ServiceError>| {
        if durable.is_ok() {
            release(credits);
        }
    })
}

fn wake_once_durable(wake: Wakes, wakes: Vec<state::schedule::Wake>) -> Finish {
    Box::new(move |durable: Result<(), ServiceError>| {
        if durable.is_ok() {
            wake(wakes);
        }
    })
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
        let taken = round.iter().fold(0_usize, |total, request| {
            total.saturating_add(request.weight())
        });
        if taken > 0 {
            links.backlog.fetch_sub(taken, Ordering::Relaxed);
        }
        now = committed_now(now);
        // A follower only applies its leader's rounds (ADR-018).
        let serving = links.role.serves();
        let mut logged = Vec::new();
        if !failed && serving {
            logged.extend(state.sweep_timeouts(now, MAX_REMOVALS_PER_ROUND));
            logged.extend(state.sweep(now, MAX_REMOVALS_PER_ROUND));
        }
        if round.is_empty() && logged.is_empty() && !state.has_credit_releases() {
            // Answers from the cold writer still turn Flows into stubs.
            state.cool();
            continue;
        }
        let mut finishes: Vec<Finish> = round
            .into_iter()
            .filter_map(|request| {
                if !serving && !request.internal() {
                    request.fail(ServiceError::Reroute);
                    return None;
                }
                handle(
                    &mut state,
                    (links.index, now),
                    request,
                    (&mut logged, &links.renew),
                )
            })
            .collect();
        // Parents of children that finished this round, on this shard.
        logged.extend(state.settle_children(now));
        // Credits of Flows that left `running` are released once durable.
        let credits = state.take_credit_releases();
        if !credits.is_empty() && serving {
            finishes.push(release_once_durable(Arc::clone(&links.release), credits));
        }
        // Queued schedules whose targets finished wake once durable.
        let wakes = state.take_schedule_wakes();
        if !wakes.is_empty() && serving {
            finishes.push(wake_once_durable(Arc::clone(&links.wake), wakes));
        }
        let pending = if failed {
            None
        } else {
            links.sink.take(&logged, sequence)
        };
        match &pending {
            Some(Pending::Appended(covered)) => sequence = *covered,
            Some(Pending::Proposed(_)) => {}
            None => failed = true,
        }
        // Flows loaded for the round go back to disk; Flows that finished
        // move there (ADR-016).
        state.cool();
        links.ready.store(state.ready(), Ordering::Relaxed);
        links.leased.store(state.leased(), Ordering::Relaxed);
        links
            .expiry
            .store(state.earliest_lease(), Ordering::Relaxed);
        for (published, count) in links.by_priority.iter().zip(state.ready_by_priority()) {
            published.store(count, Ordering::Relaxed);
        }
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
        match pending {
            Some(pending) if !failed => links.sink.release(links.index, pending, finishes),
            _ => {
                for finish in finishes {
                    finish(Err(ServiceError::Unavailable));
                }
            }
        }
    }
}
