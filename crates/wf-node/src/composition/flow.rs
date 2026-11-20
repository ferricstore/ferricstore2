//! Routes Flow commands beyond the queue fast path to their shards
//! (ADR-014): a single-Flow command to its owner, a batch split by owner and
//! joined back in request order.

use std::sync::Arc;

use wf_application::commands::flow::many::{self, ManyMutation};
use wf_application::commands::flow::{FlowCommand, FlowService, Term};
use wf_application::commands::queue::{Partition, Reply, ServiceError};

use crate::supervision::shard::Request;
use crate::supervision::shard::state::BatchItems;

use super::QueueNode;
use super::gather::ItemGather;
use super::router::Router;

impl FlowService for QueueNode {
    fn execute(&self, command: FlowCommand, reply: Reply<Term>) {
        let Some(router) = self.router.as_ref() else {
            return reply(Err(ServiceError::Unavailable));
        };
        if let FlowCommand::Many(batch) = command {
            return batch_mutation(router, *batch, reply);
        }
        let Some((id, explicit)) = command.target() else {
            return reply(Err(ServiceError::Unavailable));
        };
        let partition = Partition::for_new_flow(id, explicit);
        let index = router.route(&partition);
        router.send(
            index,
            Request::Flow {
                command: Box::new(command),
                partition,
                done: reply,
            },
        );
    }
}

fn batch_mutation(router: &Router, mut batch: ManyMutation, reply: Reply<Term>) {
    let total = batch.items.len();
    let mut per_shard: Vec<BatchItems> = (0..router.shards.len()).map(|_| Vec::new()).collect();
    for (position, mutation) in std::mem::take(&mut batch.items).into_iter().enumerate() {
        let partition = Partition::for_new_flow(&mutation.id, mutation.partition_key.as_deref());
        if let Some(items) = per_shard.get_mut(router.route(&partition)) {
            items.push((position, mutation, partition));
        }
    }
    let parts = per_shard.iter().filter(|items| !items.is_empty()).count();
    let atomic = batch.atomic();
    if atomic && parts > 1 {
        return reply(Err(ServiceError::rejected(
            "ERR flow batch crosses shards".to_owned(),
        )));
    }
    let gather = Arc::new(ItemGather::new(
        total,
        parts,
        Box::new(
            move |outcomes: Result<Vec<Result<Term, ServiceError>>, ServiceError>| {
                reply(outcomes.and_then(|outcomes| many::reply(&batch, outcomes)));
            },
        ),
    ));
    for (index, items) in per_shard
        .into_iter()
        .enumerate()
        .filter(|(_, items)| !items.is_empty())
    {
        let part = Arc::clone(&gather);
        router.send(
            index,
            Request::FlowBatch {
                items,
                atomic,
                done: Box::new(move |result| part.part(result)),
            },
        );
    }
}
