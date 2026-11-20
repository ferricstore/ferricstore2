//! Joins the per-shard parts of one request into its single reply.

use std::sync::{Mutex, PoisonError};

use wf_application::commands::queue::{ClaimedFlow, ItemOutcome, Reply, ServiceError};

struct Items {
    remaining: usize,
    outcomes: Vec<Option<ItemOutcome>>,
    failure: Option<ServiceError>,
    reply: Option<Reply<Vec<ItemOutcome>>>,
}

impl Items {
    fn place(&mut self, position: usize, outcome: ItemOutcome) {
        if let Some(slot) = self.outcomes.get_mut(position) {
            *slot = Some(outcome);
        }
    }
}

/// Per-item outcomes from several shards, put back in request order.
pub(crate) struct ItemGather(Mutex<Items>);

impl ItemGather {
    pub(crate) fn new(items: usize, parts: usize, reply: Reply<Vec<ItemOutcome>>) -> Self {
        Self(Mutex::new(Items {
            remaining: parts,
            outcomes: vec![None; items],
            failure: None,
            reply: Some(reply),
        }))
    }

    /// Records one shard's part; the last part sends the reply.
    pub(crate) fn part(&self, result: Result<Vec<(usize, ItemOutcome)>, ServiceError>) {
        let mut items = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match result {
            Ok(outcomes) => {
                for (position, outcome) in outcomes {
                    items.place(position, outcome);
                }
            }
            Err(error) => items.failure = Some(error),
        }
        items.remaining = items.remaining.saturating_sub(1);
        if items.remaining > 0 {
            return;
        }
        let Some(reply) = items.reply.take() else {
            return;
        };
        let result = match items.failure {
            Some(error) => Err(error),
            None => items
                .outcomes
                .iter()
                .map(|outcome| outcome.ok_or(ServiceError::Unavailable))
                .collect::<Result<Vec<_>, _>>(),
        };
        drop(items);
        reply(result);
    }
}

struct Claims {
    remaining: usize,
    claimed: Vec<ClaimedFlow>,
    failure: Option<ServiceError>,
    reply: Option<Reply<Vec<ClaimedFlow>>>,
}

/// Claimed Flows from several shards. A failed shard fails the reply only
/// when no other shard claimed anything, so durable claims are never hidden.
pub(crate) struct ClaimGather(Mutex<Claims>);

impl ClaimGather {
    pub(crate) fn new(parts: usize, reply: Reply<Vec<ClaimedFlow>>) -> Self {
        Self(Mutex::new(Claims {
            remaining: parts,
            claimed: Vec::new(),
            failure: None,
            reply: Some(reply),
        }))
    }

    /// Records one shard's part; the last part sends the reply.
    pub(crate) fn part(&self, result: Result<Vec<ClaimedFlow>, ServiceError>) {
        let mut claims = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match result {
            Ok(claimed) => claims.claimed.extend(claimed),
            Err(error) => claims.failure = Some(error),
        }
        claims.remaining = claims.remaining.saturating_sub(1);
        if claims.remaining > 0 {
            return;
        }
        let Some(reply) = claims.reply.take() else {
            return;
        };
        let claimed = std::mem::take(&mut claims.claimed);
        let result = match claims.failure {
            Some(error) if claimed.is_empty() => Err(error),
            _ => Ok(claimed),
        };
        drop(claims);
        reply(result);
    }
}
