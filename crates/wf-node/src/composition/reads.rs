//! Read-only commands (`FlowCommand::reads_only`) on this member's shards:
//! the leader's, or a follower's once it applied what the leader had
//! committed when the read arrived (ADR-018).

use std::time::{SystemTime, UNIX_EPOCH};

use wf_application::commands::flow::query;
use wf_application::commands::flow::{FlowCommand, Term};
use wf_application::commands::queue::Reply;

use super::router::Router;
use super::{flow, governance, info, policy};

/// Answers a read-only command from this member's shards.
pub(super) fn read(router: &Router, command: FlowCommand, reply: Reply<Term>) {
    match command {
        FlowCommand::ValueMget(read) => flow::value_read(router, read, reply),
        FlowCommand::PolicyGet(get) => policy::get(router, get, reply),
        FlowCommand::Info(read) => info::read(router, *read, reply),
        FlowCommand::Governance(command) => governance::list(router, *command, reply),
        FlowCommand::QueryIndexes(request) => {
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| {
                    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
                });
            reply(query::indexes::reply(&request, router.shards.len(), now_ms));
        }
        // A point read, history or query, on the shard its Flow or
        // partition routes to.
        command => flow::single(router, command, reply),
    }
}
