//! Joins the per-shard parts of one request into its single reply.

use std::sync::{Mutex, PoisonError};

use wf_application::commands::queue::{ItemOutcome, Reply, ServiceError};

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
