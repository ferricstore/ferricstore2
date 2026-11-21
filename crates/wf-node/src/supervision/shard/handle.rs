//! Applying one shard request (ADR-011, ADR-014): each decides its change
//! on the shard state, adds it to the round's log, and returns what to call
//! once the round is durable.

use wf_application::commands::flow::cleanup::Cleaned;
use wf_application::commands::flow::governance::{Command as GovernanceCommand, Listed};
use wf_application::commands::flow::info::{Read, Tally};
use wf_application::commands::queue::{
    CompleteFlow, ItemOutcome, NewFlow, Partition, ServiceError,
};

use super::super::completer::Finish;
use super::command::Logged;
use super::state::{self, ShardState};
use super::{Done, Request};

/// Completes fast-path Flows; outcomes keep their positions.
fn complete(
    state: &mut ShardState,
    now: u64,
    items: Vec<(usize, CompleteFlow, Partition)>,
    logged: &mut Vec<Logged>,
) -> Vec<(usize, ItemOutcome)> {
    items
        .into_iter()
        .map(|(position, completion, partition)| {
            let (outcome, change) = state.complete(now, completion, partition);
            logged.extend(change);
            (position, outcome)
        })
        .collect()
}

/// `FLOW.RETENTION_CLEANUP` on this shard; replies once durable.
fn cleanup(
    state: &mut ShardState,
    (wall, now_ms): (u64, Option<u64>),
    limit: u64,
    logged: &mut Vec<Logged>,
    done: Done<Cleaned>,
) -> Finish {
    let (cleaned, changes) = state.cleanup(wall, now_ms.unwrap_or(wall), limit);
    logged.extend(changes);
    Box::new(move |durable| done(durable.map(|()| cleaned)))
}

/// Creates fast-path Flows of one shape; outcomes keep their positions.
fn create(
    state: &mut ShardState,
    now: u64,
    shape: (&[u8], &[u8], Option<u64>, u8),
    items: Vec<(usize, NewFlow, Partition)>,
    logged: &mut Vec<Logged>,
) -> Vec<(usize, ItemOutcome)> {
    items
        .into_iter()
        .map(|(position, flow, partition)| {
            let (outcome, change) = state.create(now, shape, flow, partition);
            logged.extend(change);
            (position, outcome)
        })
        .collect()
}

/// A schedule request on this shard; replies once durable.
fn schedule(
    state: &mut ShardState,
    (op, now): (state::ScheduleOp, u64),
    logged: &mut Vec<Logged>,
    done: Done<state::ScheduleReply>,
) -> Finish {
    let (result, changes) = state.schedule_op(op, now);
    logged.extend(changes);
    Box::new(move |durable| done(durable.and(result)))
}

/// A claim, governed or not; its Flows are answered once durable.
fn claim(
    state: &mut ShardState,
    now: u64,
    request: Request,
    logged: &mut Vec<Logged>,
) -> Option<Finish> {
    let Request::Claim {
        request,
        limit,
        partitions,
        credits,
        done,
    } = request
    else {
        return None;
    };
    let taken = state.claim(
        now,
        &request,
        (limit, credits.as_deref()),
        partitions.as_deref(),
    );
    logged.extend(taken.logged);
    let claimed = taken.claimed;
    Some(Box::new(move |durable| done(durable.map(|()| claimed))))
}

/// This shard's part of a count or discovery read, once durable.
fn tally(state: &ShardState, now: u64, read: &Read, done: Done<Tally>) -> Finish {
    let tally = state.tally(read, now);
    Box::new(move |durable| done(durable.map(|()| tally)))
}

/// This shard's part of a governance listing, answered once the round's
/// writes are durable, as a count read is.
fn governance_list(
    state: &ShardState,
    now: u64,
    command: &GovernanceCommand,
    done: Done<Listed>,
) -> Finish {
    let listed = state.governance_select(command, now);
    Box::new(move |durable| done(durable.map(|()| listed)))
}

/// Applies one request; returns what to call after the round is durable, or
/// `None` when it was answered already.
pub(super) fn handle(
    state: &mut ShardState,
    (index, now): (u16, u64),
    request: Request,
    logged: &mut Vec<Logged>,
) -> Option<Finish> {
    let finish: Finish = match request {
        Request::Capture { snapshot, done } => {
            let image = state.image(now);
            logged.push(Logged::Marker {
                snapshot,
                shard: index,
            });
            Box::new(move |durable| {
                done(durable.and_then(|()| image.ok_or(ServiceError::Unavailable)));
            })
        }
        Request::Create {
            flow_type,
            state: initial,
            run_at_ms,
            priority,
            items,
            done,
        } => {
            let shape = (
                flow_type.as_slice(),
                initial.as_slice(),
                run_at_ms,
                priority,
            );
            let outcomes = create(state, now, shape, items, logged);
            Box::new(move |durable| done(durable.map(|()| outcomes)))
        }
        request @ Request::Claim { .. } => claim(state, now, request, logged)?,
        Request::Peek {
            request,
            limit,
            partitions,
            done,
        } => {
            done(state.peek(now, &request, limit, partitions.as_deref()));
            return None;
        }
        Request::Flow {
            command,
            partition,
            done,
        } => {
            let (result, change) = state.execute(now, *command, partition);
            logged.extend(change);
            Box::new(move |durable| done(durable.and(result)))
        }
        Request::Cleanup {
            now_ms,
            limit,
            done,
        } => cleanup(state, (now, now_ms), limit, logged, done),
        Request::Schedule { op, done } => schedule(state, (op, now), logged, done),
        Request::Read { read, done } => tally(state, now, &read, done),
        Request::GovernanceList { command, done } => governance_list(state, now, &command, done),
        Request::Policy { command, done } => {
            let (result, change) = state.policy_command(command);
            logged.extend(change);
            Box::new(move |durable| done(durable.and(result)))
        }
        Request::RunSteps { spec, items, done } => {
            let (outcomes, changes) = state.run_steps(now, &spec, items);
            logged.extend(changes);
            Box::new(move |durable| done(durable.map(|()| outcomes)))
        }
        Request::ValueRead {
            refs,
            max_bytes,
            done,
        } => {
            let values = state.read_values(now, refs, max_bytes);
            Box::new(move |durable| done(durable.map(|()| values)))
        }
        Request::FlowBatch {
            items,
            atomic,
            done,
        } => {
            let (replies, changes) = state.mutate_batch(now, items, atomic);
            logged.extend(changes);
            Box::new(move |durable| done(durable.map(|()| replies)))
        }
        Request::Complete { items, done } => {
            let outcomes = complete(state, now, items, logged);
            Box::new(move |durable| done(durable.map(|()| outcomes)))
        }
    };
    Some(finish)
}
