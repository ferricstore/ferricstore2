//! Joins the per-shard parts of one request into its single reply.

use std::sync::{Mutex, PoisonError};

use wf_application::commands::queue::{Reply, ServiceError};

struct Items<T> {
    remaining: usize,
    outcomes: Vec<Option<T>>,
    failure: Option<ServiceError>,
    reply: Option<Reply<Vec<T>>>,
}

impl<T> Items<T> {
    fn place(&mut self, position: usize, outcome: T) {
        if let Some(slot) = self.outcomes.get_mut(position) {
            *slot = Some(outcome);
        }
    }
}

/// Per-item outcomes from several shards, put back in request order.
pub(crate) struct ItemGather<T>(Mutex<Items<T>>);

impl<T> ItemGather<T> {
    pub(crate) fn new(items: usize, parts: usize, reply: Reply<Vec<T>>) -> Self {
        Self(Mutex::new(Items {
            remaining: parts,
            outcomes: (0..items).map(|_| None).collect(),
            failure: None,
            reply: Some(reply),
        }))
    }

    /// Records one shard's part; the last part sends the reply.
    pub(crate) fn part(&self, result: Result<Vec<(usize, T)>, ServiceError>) {
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
        let result = match items.failure.take() {
            Some(error) => Err(error),
            None => std::mem::take(&mut items.outcomes)
                .into_iter()
                .map(|outcome| outcome.ok_or(ServiceError::Unavailable))
                .collect::<Result<Vec<_>, _>>(),
        };
        drop(items);
        reply(result);
    }
}
