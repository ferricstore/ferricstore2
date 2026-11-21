//! Schedule commands across the shards (ADR-014 step 13), run on the
//! schedule thread with blocking calls: a schedule's own shard decides its
//! record; a fire reads the previous target and creates the new one on the
//! targets' shards in between, as `Schedule.fire_one/3` does.

use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use wf_application::commands::flow::FlowCommand;
use wf_application::commands::flow::schedule::definition::Definition;
use wf_application::commands::flow::schedule::list::{self, Fired};
use wf_application::commands::flow::schedule::{
    self as schedules, Command, DEFAULT_LEASE_MS, DEFAULT_LIMIT, create, flow_id,
};
use wf_application::commands::flow::term::{Term, map};
use wf_application::commands::queue::{Partition, ServiceError};

use crate::supervision::shard::Request;
use crate::supervision::shard::state::schedule::{ScheduleOp, ScheduleReply, Stored};

use super::router::Router;

mod fire;

use fire::{Outcome, fire_one};

const CALL_TIMEOUT: Duration = Duration::from_secs(30);
const WORKER: &[u8] = b"ferricstore-scheduler:ferricstore2";

/// Wall-clock milliseconds.
pub(super) fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

fn ask<T: Send + 'static>(
    router: &Router,
    index: usize,
    make: impl FnOnce(super::super::supervision::shard::Done<T>) -> Request,
) -> Result<T, ServiceError> {
    let (sender, receiver) = mpsc::channel();
    router.send(
        index,
        make(Box::new(move |result| {
            // The caller may have timed out and gone.
            let _sent = sender.send(result);
        })),
    );
    receiver
        .recv_timeout(CALL_TIMEOUT)
        .map_err(|_| ServiceError::Unavailable)?
}

fn key_of(id: &[u8]) -> (Partition, Vec<u8>) {
    (schedules::partition(id), flow_id(id))
}

fn op(
    router: &Router,
    op: ScheduleOp,
    index: Option<usize>,
) -> Result<ScheduleReply, ServiceError> {
    let index = index.unwrap_or_else(|| match &op {
        ScheduleOp::Create { key, .. }
        | ScheduleOp::Read { key }
        | ScheduleOp::Claim { key, .. }
        | ScheduleOp::Apply { key, .. }
        | ScheduleOp::Pause { key, .. }
        | ScheduleOp::Delete { key, .. } => router.route(&key.0),
        ScheduleOp::List | ScheduleOp::ClaimDue { .. } => 0,
    });
    ask(router, index, |done| Request::Schedule { op, done })
}

fn one(reply: ScheduleReply) -> Option<Stored> {
    match reply {
        ScheduleReply::One(stored) => stored.map(|stored| *stored),
        _ => None,
    }
}

/// Runs a Flow command on its Flow's shard, as the node routes it.
fn flow_call(router: &Router, command: FlowCommand) -> Result<Term, ServiceError> {
    let (id, explicit) = command.target().ok_or(ServiceError::Unavailable)?;
    let partition = Partition::for_new_flow(id, explicit);
    let index = router.route(&partition);
    ask(router, index, |done| Request::Flow {
        command: Box::new(command),
        partition,
        done,
    })
}

fn view(stored: &Stored) -> Term {
    stored.definition.view((
        &stored.key.1,
        &stored.state,
        Some(stored.version),
        Some(stored.updated_at),
        stored.attempts,
    ))
}

/// The view of a definition that was not read back, as Elixir builds the
/// replies of create and fire.
fn unread(definition: &Definition, state: &[u8]) -> Term {
    definition.view((&flow_id(&definition.id), state, None, None, 0))
}

/// Fires due schedules on every shard, at most `limit`; the summary and
/// whether anything fired or skipped.
pub(super) fn fire_due(
    router: &Router,
    now: u64,
    limit: u64,
    (worker, lease_ms): (&[u8], u64),
) -> (Term, bool) {
    let mut claimed = Vec::new();
    let mut left = usize::try_from(limit).unwrap_or(usize::MAX);
    for index in 0..router.shards.len() {
        if left == 0 {
            break;
        }
        let claim = ScheduleOp::ClaimDue {
            now,
            limit: left,
            worker: worker.to_vec(),
            lease_ms,
        };
        if let Ok(ScheduleReply::Claimed(more)) = op(router, claim, Some(index)) {
            left = left.saturating_sub(more.len());
            claimed.extend(more);
        }
    }
    // Fire in due order across the shards, as Elixir's claims return them.
    claimed.sort_by_key(|one| {
        let definition = &one.stored.definition;
        (
            definition
                .overlap_queued_due_at_ms
                .or(definition.next_run_at_ms),
            definition.id.clone(),
        )
    });
    let outcomes: Vec<Fired> = claimed
        .iter()
        .map(|one| match fire_one(router, one, None, now) {
            Ok(Outcome::Fired(target_id, coalesced, _)) => Fired::Fired {
                target_id,
                coalesced,
            },
            Ok(Outcome::Skipped(reason, coalesced, _)) => Fired::Skipped { reason, coalesced },
            Err(error) => Fired::Failed {
                id: one.stored.definition.id.clone(),
                reason: error.to_string(),
            },
        })
        .collect();
    let active = outcomes
        .iter()
        .any(|outcome| !matches!(outcome, Fired::Failed { .. }));
    (list::summary(&outcomes), active)
}

/// Runs one schedule command.
pub(super) fn run(router: &Router, command: Command) -> Result<Term, ServiceError> {
    let wall = wall_ms();
    match command {
        Command::Create(request) => {
            let definition = create::definition(&request, wall)?;
            let now = definition.created_at_ms;
            let key = key_of(&request.id);
            let created = ScheduleOp::Create {
                key,
                definition: Box::new(definition),
                overwrite: request.overwrite,
                now,
            };
            let stored = one(op(router, created, None)?).ok_or(ServiceError::Unavailable)?;
            Ok(unread(&stored.definition, b"active"))
        }
        Command::Get(target) => {
            let reply = op(
                router,
                ScheduleOp::Read {
                    key: key_of(&target.id),
                },
                None,
            )?;
            Ok(one(reply).map_or(Term::Nil, |stored| view(&stored)))
        }
        Command::Delete(target) => {
            let now = target.now_ms.unwrap_or(wall);
            op(
                router,
                ScheduleOp::Delete {
                    key: key_of(&target.id),
                    now,
                },
                None,
            )?;
            Ok(Term::text("OK"))
        }
        Command::Pause(target) => pause(router, &target, true, wall),
        Command::Resume(target) => pause(router, &target, false, wall),
        Command::Fire(target, fire_at) => {
            let now = target.now_ms.unwrap_or(wall);
            let claim = ScheduleOp::Claim {
                key: key_of(&target.id),
                now,
                worker: manual_worker(),
                lease_ms: DEFAULT_LEASE_MS,
            };
            let ScheduleReply::Claimed(claimed) = op(router, claim, None)? else {
                return Err(ServiceError::Unavailable);
            };
            let claimed = claimed.first().ok_or(ServiceError::Unavailable)?;
            match fire_one(router, claimed, Some(fire_at.unwrap_or(now)), now)? {
                Outcome::Fired(target_id, _, stored) => Ok(map(vec![
                    ("fired", Term::Int(1)),
                    ("schedule", unread(&stored.definition, &stored.state)),
                    ("target_id", Term::Bin(target_id)),
                ])),
                Outcome::Skipped(reason, _, stored) => Ok(map(vec![
                    ("fired", Term::Int(0)),
                    ("reason", Term::text(&reason)),
                    ("schedule", unread(&stored.definition, &stored.state)),
                    ("skipped", Term::Int(1)),
                ])),
            }
        }
        Command::FireDue(request) => {
            let now = request.now_ms.unwrap_or(wall);
            Ok(fire_due(
                router,
                now,
                request.limit,
                (&request.worker, request.lease_ms),
            )
            .0)
        }
        Command::List(listing) => {
            let mut views = Vec::new();
            for index in 0..router.shards.len() {
                if let ScheduleReply::Many(stored) = op(router, ScheduleOp::List, Some(index))? {
                    views.extend(stored.iter().map(view));
                }
            }
            Ok(listing.select(views))
        }
    }
}

fn manual_worker() -> Vec<u8> {
    let mut worker = WORKER.to_vec();
    worker.extend_from_slice(b":manual");
    worker
}

fn pause(
    router: &Router,
    target: &schedules::Target,
    pause: bool,
    wall: u64,
) -> Result<Term, ServiceError> {
    let now = target.now_ms.unwrap_or(wall);
    let reply = op(
        router,
        ScheduleOp::Pause {
            key: key_of(&target.id),
            pause,
            now,
        },
        None,
    )?;
    let stored = one(reply).ok_or(ServiceError::Unavailable)?;
    Ok(unread(&stored.definition, &stored.state))
}

/// The background poll: fires due schedules at wall time; whether it did.
pub(super) fn poll(router: &Router) -> bool {
    fire_due(router, wall_ms(), DEFAULT_LIMIT, (WORKER, DEFAULT_LEASE_MS)).1
}
