//! Routes Flow commands beyond the queue fast path to their shards
//! (ADR-014): a single-Flow command to its owner, a batch split by owner and
//! joined back in request order.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use wf_application::commands::flow::keys;
use wf_application::commands::flow::many::{self, ManyMutation};
use wf_application::commands::flow::start::RunSteps;
use wf_application::commands::flow::value::{self, ValueMget};
use wf_application::commands::flow::{FlowCommand, FlowService, Term};
use wf_application::commands::queue::{
    Partition, Reply, ServiceError, shard_for_slot, slot_for_tag,
};

use crate::supervision::shard::Request;
use crate::supervision::shard::state::BatchItems;

use super::QueueNode;
use super::gather::ItemGather;
use super::router::Router;
use super::{cleanup, policy};

impl FlowService for QueueNode {
    fn execute(&self, command: FlowCommand, reply: Reply<Term>) {
        let Some(router) = self.router.as_ref() else {
            return reply(Err(ServiceError::Unavailable));
        };
        let command = match command {
            FlowCommand::Many(batch) => return batch_mutation(router, *batch, reply),
            FlowCommand::ValueMget(read) => return value_read(router, read, reply),
            FlowCommand::RunSteps(spec) => return run_steps(router, *spec, reply),
            FlowCommand::PolicySet(set) => return policy::set(router, set, reply),
            FlowCommand::PolicyGet(get) => return policy::get(router, get, reply),
            FlowCommand::RetentionCleanup(request) => return cleanup::run(router, &request, reply),
            FlowCommand::SpawnChildren(spawn) => {
                // A spawn is decided on the parent's shard; children placed
                // on other shards need a cross-shard spawn, not built yet.
                let parent = router.route(&Partition::parse(&spawn.partition_key));
                let elsewhere = spawn.children.iter().any(|child| {
                    let partition =
                        Partition::for_new_flow(&child.id, child.partition_key.as_deref());
                    router.route(&partition) != parent
                });
                if elsewhere {
                    return reply(Err(ServiceError::rejected(
                        "ERR flow children on another shard than their parent are not supported by this server yet"
                            .to_owned(),
                    )));
                }
                FlowCommand::SpawnChildren(spawn)
            }
            FlowCommand::ValuePut(mut put) if put.named.is_none() => {
                // A shared value's reference is chosen here, so its auto
                // bucket is known before routing.
                let id = value::shared_id(&entropy());
                let partition = Partition::for_new_flow(&id, put.partition_key.as_deref());
                put.shared_ref = Some(keys::value_ref(&partition, 's', &id, 1));
                let index = router.route(&partition);
                return router.send(
                    index,
                    Request::Flow {
                        command: Box::new(FlowCommand::ValuePut(put)),
                        partition,
                        done: reply,
                    },
                );
            }
            command => command,
        };
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

/// Seed of a shared value id: process id, wall-clock nanoseconds and a
/// process-wide counter, so ids never repeat within a node.
fn entropy() -> Vec<u8> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let mut seed = std::process::id().to_be_bytes().to_vec();
    seed.extend_from_slice(&nanos.to_be_bytes());
    seed.extend_from_slice(&NEXT.fetch_add(1, Ordering::Relaxed).to_be_bytes());
    seed
}

/// Reads values by reference: each reference goes to the shard its hash tag
/// routes to; one that is not a value reference reads as nil.
fn value_read(router: &Router, read: ValueMget, reply: Reply<Term>) {
    let total = read.refs.len();
    let mut per_shard: Vec<Vec<(usize, Vec<u8>)>> =
        (0..router.shards.len()).map(|_| Vec::new()).collect();
    let mut misses = Vec::new();
    for (position, reference) in read.refs.into_iter().enumerate() {
        let shard = value::parse_ref(&reference)
            .and_then(|parsed| shard_for_slot(slot_for_tag(&parsed.tag), router.shard_count))
            .map(usize::from);
        match shard.and_then(|index| per_shard.get_mut(index)) {
            Some(items) => items.push((position, reference)),
            None => misses.push((position, Term::Nil)),
        }
    }
    let parts = per_shard.iter().filter(|items| !items.is_empty()).count();
    let gather = Arc::new(ItemGather::new(
        total,
        parts.saturating_add(1),
        Box::new(move |values: Result<Vec<Term>, ServiceError>| reply(values.map(Term::List))),
    ));
    gather.part(Ok(misses));
    for (index, refs) in per_shard
        .into_iter()
        .enumerate()
        .filter(|(_, refs)| !refs.is_empty())
    {
        let part = Arc::clone(&gather);
        router.send(
            index,
            Request::ValueRead {
                refs,
                max_bytes: read.max_bytes,
                done: Box::new(move |result| part.part(result)),
            },
        );
    }
}

/// Runs step chains, each on the shard owning its Flow; the reply is `OK`
/// when every item succeeded, otherwise one `ok` or `[error, message]` per
/// item.
fn run_steps(router: &Router, mut spec: RunSteps, reply: Reply<Term>) {
    let items = std::mem::take(&mut spec.items);
    let total = items.len();
    let mut seen: HashSet<(&[u8], Option<&[u8]>)> = HashSet::with_capacity(total);
    let duplicate = !items
        .iter()
        .all(|(id, partition)| seen.insert((id.as_slice(), partition.as_deref())));
    if duplicate {
        let error = Term::List(vec![
            Term::text("error"),
            Term::text("ERR flow duplicate id in batch"),
        ]);
        return reply(Ok(Term::List(vec![error; total])));
    }
    let mut per_shard: Vec<Vec<(usize, Vec<u8>, Partition)>> =
        (0..router.shards.len()).map(|_| Vec::new()).collect();
    for (position, (id, explicit)) in items.into_iter().enumerate() {
        let partition = Partition::for_new_flow(&id, explicit.as_deref());
        if let Some(items) = per_shard.get_mut(router.route(&partition)) {
            items.push((position, id, partition));
        }
    }
    let parts = per_shard.iter().filter(|items| !items.is_empty()).count();
    let gather = Arc::new(ItemGather::new(
        total,
        parts,
        Box::new(
            move |outcomes: Result<Vec<Result<Term, ServiceError>>, ServiceError>| {
                reply(outcomes.map(|outcomes| {
                    if outcomes.iter().all(Result::is_ok) {
                        return Term::text("OK");
                    }
                    Term::List(
                        outcomes
                            .into_iter()
                            .map(|outcome| match outcome {
                                Ok(_) => Term::text("ok"),
                                Err(error) => Term::List(vec![
                                    Term::text("error"),
                                    Term::Bin(error.to_string().into_bytes()),
                                ]),
                            })
                            .collect(),
                    )
                }));
            },
        ),
    ));
    let spec = Arc::new(spec);
    for (index, items) in per_shard
        .into_iter()
        .enumerate()
        .filter(|(_, items)| !items.is_empty())
    {
        let part = Arc::clone(&gather);
        router.send(
            index,
            Request::RunSteps {
                spec: Arc::clone(&spec),
                items,
                done: Box::new(move |result| part.part(result)),
            },
        );
    }
}
