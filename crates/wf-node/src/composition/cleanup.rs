//! `FLOW.RETENTION_CLEANUP` across the shards (ADR-014 step 9): one shard
//! at a time from the continuation's shard, each given what is left of the
//! limit; when the limit runs out the reply names the shard to resume at.

use std::sync::Arc;

use wf_application::commands::flow::Term;
use wf_application::commands::flow::cleanup::{self, Cleaned, RetentionCleanup};
use wf_application::commands::queue::{Reply, ServiceError};

use crate::supervision::shard::Request;

use super::router::Router;

/// Runs a cleanup from `request.from_shard`.
pub(super) fn run(router: &Arc<Router>, request: &RetentionCleanup, reply: Reply<Term>) {
    step(
        router,
        (request.from_shard, request.limit),
        (request.now_ms, Cleaned::default()),
        reply,
    );
}

fn step(
    router: &Arc<Router>,
    (shard, left): (u16, u64),
    (now_ms, total): (Option<u64>, Cleaned),
    reply: Reply<Term>,
) {
    if usize::from(shard) >= router.shards.len() {
        return reply(Ok(cleanup::reply(total, None)));
    }
    let next = Arc::clone(router);
    router.send(
        usize::from(shard),
        Request::Cleanup {
            now_ms,
            limit: left,
            done: Box::new(move |result: Result<Cleaned, ServiceError>| match result {
                Ok(cleaned) => {
                    let more = cleaned.more;
                    let total = total.plus(cleaned);
                    let left = left.saturating_sub(cleaned.taken());
                    if left == 0 && more {
                        reply(Ok(cleanup::reply(total, Some(shard))));
                    } else {
                        // With the limit used up, later shards are asked
                        // with none left: the first with work due is where
                        // the continuation resumes (audit gap G8).
                        step(
                            &next,
                            (shard.saturating_add(1), left),
                            (now_ms, total),
                            reply,
                        );
                    }
                }
                Err(error) => reply(Err(error)),
            }),
        },
    );
}
