//! A claim that reclaims expired leases by ratio (audit gap G24), in
//! Elixir's three passes over every shard: expired leases up to the ratio
//! of the limit, then due Flows for the rest, then expired leases for what
//! is still left. Splitting the limit over shards first would let each
//! shard apply the ratio to its own small share, where a share of one takes
//! due work and reclaims nothing.

use std::sync::Arc;

use wf_application::commands::queue::{ClaimFlows, ClaimedFlow, Reply, ServiceError};

use super::super::router::Router;
use super::start;
use crate::supervision::shard::state::credit::CreditPool;

/// Whether `request` is claimed in passes: it reclaims by a ratio, may take
/// more than one Flow, takes due work too, does not wait, and some lease
/// has expired (otherwise one pass takes the same Flows).
pub(super) fn applies(router: &Router, request: &ClaimFlows) -> bool {
    request.reclaim.is_some_and(|ratio| ratio > 0)
        && request.limit > 1
        && !request.running_only()
        && request.block_ms.is_none()
        && router.any_expired(request.now_ms.unwrap_or_else(wall_ms))
}

fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// `request` limited to `limit` expired leases, or to due Flows.
fn pass(request: &ClaimFlows, limit: usize, expired: bool) -> ClaimFlows {
    let mut pass = request.clone();
    pass.limit = limit;
    pass.reclaim = None;
    if expired {
        pass.states = vec![b"running".to_vec()];
    }
    pass
}

/// A claim between its passes.
struct Phases {
    router: Arc<Router>,
    request: ClaimFlows,
    credits: Option<Arc<CreditPool>>,
    reply: Reply<Vec<ClaimedFlow>>,
    taken: Vec<ClaimedFlow>,
}

/// The pass after the one running.
#[derive(Clone, Copy)]
enum Next {
    Due,
    MoreExpired,
    Done,
}

/// Runs the three passes and replies with every Flow they took.
pub(super) fn claim(
    router: &Arc<Router>,
    request: ClaimFlows,
    credits: Option<Arc<CreditPool>>,
    reply: Reply<Vec<ClaimedFlow>>,
) {
    let ratio = usize::from(request.reclaim.unwrap_or(0));
    let first = request
        .limit
        .saturating_mul(ratio)
        .saturating_add(99)
        .checked_div(100)
        .unwrap_or(0)
        .max(1);
    let phases = Box::new(Phases {
        router: Arc::clone(router),
        request,
        credits,
        reply,
        taken: Vec::new(),
    });
    run(phases, (first, true), Next::Due);
}

fn run(phases: Box<Phases>, (limit, expired): (usize, bool), next: Next) {
    let router = Arc::clone(&phases.router);
    let request = pass(&phases.request, limit, expired);
    let credits = phases.credits.clone();
    start(
        &router,
        request,
        credits,
        Box::new(move |result| after(phases, result, next)),
    );
}

fn after(mut phases: Box<Phases>, result: Result<Vec<ClaimedFlow>, ServiceError>, next: Next) {
    match result {
        Ok(taken) => phases.taken.extend(taken),
        Err(error) => return (phases.reply)(Err(error)),
    }
    let left = phases.request.limit.saturating_sub(phases.taken.len());
    match next {
        Next::Due if left > 0 => run(phases, (left, false), Next::MoreExpired),
        Next::MoreExpired if left > 0 => run(phases, (left, true), Next::Done),
        _ => {
            let Phases { reply, taken, .. } = *phases;
            reply(Ok(taken));
        }
    }
}
