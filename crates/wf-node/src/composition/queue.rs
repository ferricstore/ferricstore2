//! The queue fast path's service: creates and completions split by shard
//! and joined back in request order, and claims (ADR-011, ADR-013).

use std::sync::Arc;

use wf_application::commands::queue::{
    ClaimFlows, ClaimedFlow, CompleteFlow, CreateFlows, ItemOutcome, QueueService, Reply,
    ServiceError,
};

use crate::supervision::shard::Request;

use super::QueueNode;
use super::admission::{Class, occupied};
use super::gather::ItemGather;

impl QueueService for QueueNode {
    fn create(&self, request: CreateFlows, reply: Reply<Vec<ItemOutcome>>) {
        let Some(router) = self.router.as_ref() else {
            return reply(Err(ServiceError::Unavailable));
        };
        if let Err(error) = router.serving() {
            return reply(Err(error));
        }
        let CreateFlows {
            flow_type,
            state,
            run_at_ms,
            priority,
            flows,
        } = request;
        let total = flows.len();
        let per_shard = Self::per_shard(
            router,
            flows
                .into_iter()
                .map(|flow| (flow.id.clone(), flow.partition.clone(), flow)),
        );
        let parts = per_shard.iter().filter(|items| !items.is_empty()).count();
        if parts == 0 {
            reply(Ok(Vec::new()));
            return;
        }
        if let Err(error) = router.admit(occupied(&per_shard), Class::New) {
            return reply(Err(error));
        }
        let gather = Arc::new(ItemGather::new(total, parts, reply));
        for (index, items) in per_shard
            .into_iter()
            .enumerate()
            .filter(|(_, items)| !items.is_empty())
        {
            let part = Arc::clone(&gather);
            router.send(
                index,
                Request::Create {
                    flow_type: flow_type.clone(),
                    state: state.clone(),
                    run_at_ms,
                    priority,
                    items,
                    done: Box::new(move |result| part.part(result)),
                },
            );
        }
    }

    fn claim(&self, request: ClaimFlows, reply: Reply<Vec<ClaimedFlow>>) {
        match self.router.as_ref() {
            Some(router) if router.serving().is_err() => {
                reply(router.serving().map(|()| Vec::new()));
            }
            Some(router) => self.governed_claim(router, request, reply),
            None => reply(Err(ServiceError::Unavailable)),
        }
    }

    fn complete(&self, completions: Vec<CompleteFlow>, reply: Reply<Vec<ItemOutcome>>) {
        let Some(router) = self.router.as_ref() else {
            return reply(Err(ServiceError::Unavailable));
        };
        if let Err(error) = router.serving() {
            return reply(Err(error));
        }
        let total = completions.len();
        let per_shard = Self::per_shard(
            router,
            completions.into_iter().map(|completion| {
                (
                    completion.id.clone(),
                    completion.partition.clone(),
                    completion,
                )
            }),
        );
        let parts = per_shard.iter().filter(|items| !items.is_empty()).count();
        if parts == 0 {
            reply(Ok(Vec::new()));
            return;
        }
        if let Err(error) = router.admit(occupied(&per_shard), Class::Other) {
            return reply(Err(error));
        }
        let gather = Arc::new(ItemGather::new(total, parts, reply));
        for (index, items) in per_shard
            .into_iter()
            .enumerate()
            .filter(|(_, items)| !items.is_empty())
        {
            let part = Arc::clone(&gather);
            router.send(
                index,
                Request::Complete {
                    items,
                    done: Box::new(move |result| part.part(result)),
                },
            );
        }
    }
}
