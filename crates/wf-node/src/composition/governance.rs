//! Governance listings across the shards (ADR-014 step 15): every shard
//! selects its matching records and the parts are joined into one reply.

use std::sync::Arc;

use wf_application::commands::flow::Term;
use wf_application::commands::flow::governance::{Command, Listed};
use wf_application::commands::queue::{Reply, ServiceError};

use crate::supervision::shard::Request;

use super::gather::ItemGather;
use super::router::Router;

/// Answers a listing from every shard.
pub(super) fn list(router: &Router, command: Command, reply: Reply<Term>) {
    let shards = router.shards.len();
    let command = Arc::new(command);
    let answer = Arc::clone(&command);
    let gather = Arc::new(ItemGather::new(
        shards,
        shards,
        Box::new(move |parts: Result<Vec<Listed>, ServiceError>| {
            reply(parts.map(|parts| answer.join(parts.into_iter().flatten().collect())));
        }),
    ));
    for index in 0..shards {
        let part = Arc::clone(&gather);
        router.send(
            index,
            Request::GovernanceList {
                command: Arc::clone(&command),
                done: Box::new(move |result: Result<Listed, ServiceError>| {
                    part.part(result.map(|records| vec![(index, records)]));
                }),
            },
        );
    }
}
