//! One claim from request to reply (ADR-013).
//!
//! An attempt starts with the split weighted by indexed counts. When that
//! returns fewer Flows than the limit, the shards that may still have some
//! (not asked yet, or filled their share) count what they could claim now; the
//! rest of the limit is split by those counts and claimed, for at most
//! [`MAX_ROUNDS`] claim rounds. A blocking claim that found nothing parks in
//! the waiter thread and starts a new attempt when woken.

use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::{Duration, Instant};

use wf_application::commands::queue::{ClaimFlows, ClaimedFlow, Partition, Reply, ServiceError};

use crate::supervision::shard::Request;
use crate::supervision::shard::state::Peeked;
use crate::supervision::shard::state::credit::CreditPool;
use crate::supervision::waiters::{Parked, Wake};

use super::admission::Class;
use super::router::{Router, split};

/// Most claim rounds in one attempt.
const MAX_ROUNDS: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Blocking {
    No,
    Until(Instant),
    Forever,
}

struct ClaimRun {
    router: Weak<Router>,
    request: Arc<ClaimFlows>,
    partitions: Option<Arc<[Partition]>>,
    blocking: Blocking,
    reply: Reply<Vec<ClaimedFlow>>,
    claimed: Vec<ClaimedFlow>,
    failure: Option<ServiceError>,
    epoch: u64,
    rounds: u8,
    /// Shards known to have nothing more for this attempt.
    exhausted: Vec<bool>,
    /// Whether shards reported their next due times in this attempt.
    timed: bool,
    wait_ms: Option<u64>,
    /// A governed claim's credits (ADR-014 step 15).
    credits: Option<Arc<CreditPool>>,
}

/// Starts a claim, stamping each Flow taken with one of `credits` when
/// governed; `reply` is called once.
pub(crate) fn start(
    router: &Arc<Router>,
    request: ClaimFlows,
    credits: Option<Arc<CreditPool>>,
    reply: Reply<Vec<ClaimedFlow>>,
) {
    let partitions: Option<Arc<[Partition]>> = request
        .partitions
        .as_ref()
        .map(|names| names.iter().map(|name| Partition::parse(name)).collect());
    let holders = router.holders(&request, partitions.as_deref());
    let shards = holders.iter().map(|(index, _)| *index);
    if let Err(error) = router.admit(shards, Class::Other) {
        return reply(Err(error));
    }
    let blocking = match request.block_ms {
        None => Blocking::No,
        Some(0) => Blocking::Forever,
        Some(ms) => Instant::now()
            .checked_add(Duration::from_millis(ms))
            .map_or(Blocking::Forever, Blocking::Until),
    };
    let run = Box::new(ClaimRun {
        router: Arc::downgrade(router),
        request: Arc::new(request),
        partitions,
        blocking,
        reply,
        claimed: Vec::new(),
        failure: None,
        epoch: 0,
        rounds: 0,
        exhausted: Vec::new(),
        timed: false,
        wait_ms: None,
        credits,
    });
    attempt(router, run);
}

fn attempt(router: &Router, mut run: Box<ClaimRun>) {
    run.epoch = router.waiters.epoch();
    run.rounds = 0;
    run.timed = false;
    run.wait_ms = None;
    run.exhausted = vec![false; router.shards.len()];
    let holders = router.holders(&run.request, run.partitions.as_deref());
    if holders.is_empty() {
        return settle(router, run);
    }
    let shares = split(run.request.limit, &holders);
    claim_round(router, run, shares);
}

/// Parts of one round; the last part continues the claim.
struct Round<T> {
    remaining: usize,
    parts: Vec<Part<T>>,
    run: Option<Box<ClaimRun>>,
}

type Gather<T> = Arc<Mutex<Round<T>>>;

fn gather<T>(run: Box<ClaimRun>, parts: usize) -> Gather<T> {
    Arc::new(Mutex::new(Round {
        remaining: parts,
        parts: Vec::with_capacity(parts),
        run: Some(run),
    }))
}

/// A shard index, the share it was asked for, and its answer.
type Part<T> = (usize, usize, T);

/// Records one part; returns the run and every part once all have arrived.
fn arrive<T>(round: &Gather<T>, part: Part<T>) -> Option<(Box<ClaimRun>, Vec<Part<T>>)> {
    let mut round = round.lock().unwrap_or_else(PoisonError::into_inner);
    round.parts.push(part);
    round.remaining = round.remaining.saturating_sub(1);
    if round.remaining > 0 {
        return None;
    }
    let run = round.run.take()?;
    Some((run, std::mem::take(&mut round.parts)))
}

fn claim_round(router: &Router, run: Box<ClaimRun>, shares: Vec<(usize, usize)>) {
    let request = Arc::clone(&run.request);
    let partitions = run.partitions.clone();
    let credits = run.credits.clone();
    let round: Gather<Result<Vec<ClaimedFlow>, ServiceError>> = gather(run, shares.len());
    for (index, limit) in shares {
        let part = Arc::clone(&round);
        router.send(
            index,
            Request::Claim {
                request: Arc::clone(&request),
                limit,
                partitions: partitions.clone(),
                credits: credits.clone(),
                done: Box::new(move |result| {
                    if let Some((run, parts)) = arrive(&part, (index, limit, result)) {
                        claimed(run, parts);
                    }
                }),
            },
        );
    }
}

fn claimed(
    mut run: Box<ClaimRun>,
    parts: Vec<(usize, usize, Result<Vec<ClaimedFlow>, ServiceError>)>,
) {
    for (index, share, result) in parts {
        let short = match result {
            Ok(flows) => {
                let short = flows.len() < share;
                run.claimed.extend(flows);
                short
            }
            Err(error) => {
                run.failure = Some(error);
                true
            }
        };
        if short && let Some(exhausted) = run.exhausted.get_mut(index) {
            *exhausted = true;
        }
    }
    run.rounds = run.rounds.saturating_add(1);
    if run.claimed.len() >= run.request.limit {
        return finish(*run);
    }
    match run.router.upgrade() {
        Some(router) => peek(&router, run),
        None => fail(*run, ServiceError::Unavailable),
    }
}

/// Asks the shards that may still have Flows what they could claim now. A
/// blocking claim with nothing yet asks every holder once, for due times.
fn peek(router: &Router, run: Box<ClaimRun>) {
    let holders: Vec<usize> = router
        .holders(&run.request, run.partitions.as_deref())
        .into_iter()
        .map(|(index, _)| index)
        .collect();
    let mut candidates: Vec<usize> = holders
        .iter()
        .copied()
        .filter(|index| run.exhausted.get(*index) == Some(&false))
        .collect();
    if candidates.is_empty() && run.blocking != Blocking::No && run.claimed.is_empty() && !run.timed
    {
        candidates = holders;
    }
    if candidates.is_empty() {
        return settle(router, run);
    }
    let remaining = run.request.limit.saturating_sub(run.claimed.len());
    let request = Arc::clone(&run.request);
    let partitions = run.partitions.clone();
    let round: Gather<Peeked> = gather(run, candidates.len());
    for index in candidates {
        let part = Arc::clone(&round);
        router.send(
            index,
            Request::Peek {
                request: Arc::clone(&request),
                limit: remaining,
                partitions: partitions.clone(),
                done: Box::new(move |peeked| {
                    if let Some((run, parts)) = arrive(&part, (index, 0, peeked)) {
                        peeked_round(run, &parts);
                    }
                }),
            },
        );
    }
}

fn peeked_round(mut run: Box<ClaimRun>, parts: &[(usize, usize, Peeked)]) {
    run.timed = true;
    run.wait_ms = parts
        .iter()
        .filter_map(|(_, _, peeked)| peeked.wait_ms)
        .min();
    for (index, _, peeked) in parts {
        if peeked.available == 0
            && let Some(exhausted) = run.exhausted.get_mut(*index)
        {
            *exhausted = true;
        }
    }
    let counts: Vec<(usize, u64)> = parts
        .iter()
        .filter(|(_, _, peeked)| peeked.available > 0)
        .filter_map(|(index, _, peeked)| Some((*index, u64::try_from(peeked.available).ok()?)))
        .collect();
    let Some(router) = run.router.upgrade() else {
        return fail(*run, ServiceError::Unavailable);
    };
    if counts.is_empty() {
        return settle(&router, run);
    }
    if run.rounds >= MAX_ROUNDS {
        // Flows are claimable but other claims keep taking them first.
        run.wait_ms = Some(0);
        return settle(&router, run);
    }
    let remaining = run.request.limit.saturating_sub(run.claimed.len());
    let shares = split(remaining, &counts)
        .into_iter()
        .map(|(index, share)| {
            let available = counts
                .iter()
                .find(|(counted, _)| *counted == index)
                .and_then(|(_, count)| usize::try_from(*count).ok())
                .unwrap_or(0);
            (index, share.min(available))
        })
        .filter(|(_, share)| *share > 0)
        .collect();
    claim_round(&router, run, shares);
}

/// Replies, or parks a blocking claim that found nothing.
fn settle(router: &Router, run: Box<ClaimRun>) {
    if !run.claimed.is_empty() || run.failure.is_some() {
        return finish(*run);
    }
    let deadline = match run.blocking {
        Blocking::No => return finish(*run),
        Blocking::Until(deadline) if deadline <= Instant::now() => return finish(*run),
        Blocking::Until(deadline) => Some(deadline),
        Blocking::Forever => None,
    };
    let due = run
        .wait_ms
        .and_then(|ms| Instant::now().checked_add(Duration::from_millis(ms)));
    let parked = Parked {
        flow_type: run.request.flow_type.clone(),
        states: run.request.states.clone(),
        priority: run.request.priority,
        epoch: run.epoch,
        deadline,
        due,
        cancellation: run.request.cancellation.clone(),
        resume: Box::new(move |wake| resumed(run, wake)),
    };
    router.waiters.park(parked);
}

fn resumed(run: Box<ClaimRun>, wake: Wake) {
    match wake {
        Wake::Retry => match run.router.upgrade() {
            Some(router) => attempt(&router, run),
            None => fail(*run, ServiceError::Unavailable),
        },
        Wake::Expired | Wake::Cancelled => finish(*run),
        Wake::Stopped => fail(*run, ServiceError::Unavailable),
        Wake::Full => fail(*run, ServiceError::TooManyWaiting),
    }
}

fn fail(mut run: ClaimRun, error: ServiceError) {
    run.failure = Some(error);
    finish(run);
}

/// A failure fails the reply only when nothing was claimed, so durable
/// claims are never hidden.
fn finish(run: ClaimRun) {
    let ClaimRun {
        reply,
        claimed,
        failure,
        ..
    } = run;
    match failure {
        Some(error) if claimed.is_empty() => reply(Err(error)),
        _ => reply(Ok(claimed)),
    }
}
