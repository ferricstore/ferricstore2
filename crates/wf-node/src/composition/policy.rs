//! Routes policy commands (ADR-014 step 9). Shard 0 owns every type's
//! policy, as in Elixir: a get reads it there and a set is decided and logged
//! there. Once the set is durable, every other shard installs a copy before
//! the client hears back, so a command that follows the reply on any shard
//! sees the new policy.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use wf_application::commands::flow::Term;
use wf_application::commands::flow::policy::{Installed, PolicyGet, PolicySet};
use wf_application::commands::queue::{Reply, ServiceError};

use crate::supervision::shard::Request;
use crate::supervision::shard::state::{PolicyCommand, PolicyReply};

use super::router::Router;

/// Shard 0, where policies are decided.
const OWNER: usize = 0;

/// `FLOW.POLICY.GET`, answered from shard 0's table.
pub(super) fn get(router: &Router, get: PolicyGet, reply: Reply<Term>) {
    router.send(
        OWNER,
        Request::Policy {
            command: PolicyCommand::Get(get),
            done: Box::new(move |result: Result<PolicyReply, ServiceError>| {
                reply(result.map(|(term, _)| term));
            }),
        },
    );
}

/// `FLOW.POLICY.SET`: decided on shard 0, then copied to the others.
pub(super) fn set(router: &Arc<Router>, set: Box<PolicySet>, reply: Reply<Term>) {
    let shards = Arc::clone(router);
    router.send(
        OWNER,
        Request::Policy {
            command: PolicyCommand::Set(set),
            done: Box::new(
                move |result: Result<PolicyReply, ServiceError>| match result {
                    Ok((term, Some(installed))) => copy(&shards, &installed, term, reply),
                    Ok((term, None)) => reply(Ok(term)),
                    Err(error) => reply(Err(error)),
                },
            ),
        },
    );
}

/// Installs shard 0's durable set on every other shard, then replies.
fn copy(router: &Router, installed: &Arc<Installed>, term: Term, reply: Reply<Term>) {
    let others: Vec<usize> = (0..router.shards.len())
        .filter(|index| *index != OWNER)
        .collect();
    if others.is_empty() {
        return reply(Ok(term));
    }
    let remaining = Arc::new(AtomicUsize::new(others.len()));
    let waiting = Arc::new(Mutex::new(Some((reply, term))));
    for index in others {
        let remaining = Arc::clone(&remaining);
        let waiting = Arc::clone(&waiting);
        // A shard that cannot install still answers; shard 0 holds the
        // policy durably and every shard copies it again at open.
        let done = move |_installed: Result<PolicyReply, ServiceError>| {
            if remaining.fetch_sub(1, Ordering::AcqRel) != 1 {
                return;
            }
            let taken = waiting
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            if let Some((reply, term)) = taken {
                reply(Ok(term));
            }
        };
        router.send(
            index,
            Request::Policy {
                command: PolicyCommand::Install(Arc::clone(installed)),
                done: Box::new(done),
            },
        );
    }
}
