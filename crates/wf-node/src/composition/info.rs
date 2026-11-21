//! Count and discovery reads across the shards (ADR-014 step 11): a read
//! of one partition goes to the shard that owns it; a read of the auto
//! buckets goes to every shard, and the parts are joined into one reply.

use std::sync::Arc;

use wf_application::commands::flow::Term;
use wf_application::commands::flow::info::{self, Read, Scope, Tally};
use wf_application::commands::queue::{Reply, ServiceError};

use crate::supervision::shard::Request;

use super::gather::ItemGather;
use super::router::Router;

/// Answers `read` from the shards it covers.
pub(super) fn read(router: &Router, read: Read, reply: Reply<Term>) {
    let shards: Vec<usize> = match &read.scope {
        Scope::Partition(partition) => vec![router.route(partition)],
        Scope::Auto => (0..router.shards.len()).collect(),
    };
    let read = Arc::new(read);
    let answer = Arc::clone(&read);
    let gather = Arc::new(ItemGather::new(
        shards.len(),
        shards.len(),
        Box::new(move |parts: Result<Vec<Tally>, ServiceError>| {
            reply(parts.and_then(|parts| {
                parts
                    .into_iter()
                    .reduce(Tally::merge)
                    .map(|tally| info::reply(&answer, tally))
                    .ok_or(ServiceError::Unavailable)
            }));
        }),
    ));
    for (position, index) in shards.into_iter().enumerate() {
        let part = Arc::clone(&gather);
        router.send(
            index,
            Request::Read {
                read: Arc::clone(&read),
                done: Box::new(move |result: Result<Tally, ServiceError>| {
                    part.part(result.map(|tally| vec![(position, tally)]));
                }),
            },
        );
    }
}
