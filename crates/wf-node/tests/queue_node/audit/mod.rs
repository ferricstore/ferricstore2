//! Ports of the Elixir Flow business tests for ADR-014 steps 1 to 10 (the
//! audit after step 15). Each test names its Elixir source file and test;
//! the map of every Elixir test and its status is
//! `docs/reviews/workflow-flow-test-audit.md`.
//!
//! Errors are compared by their reply text, which is what a client sees.

use wf_application::commands::flow::{self, FlowService, Term, claim};
use wf_application::commands::queue::{QueueService, ServiceError};
use wf_node::composition::QueueNode;

use super::{TestResult, call};

mod active_timeout_children;
mod active_timeouts;
mod attribute_queries;
mod attributes;
mod batch_indexing;
mod batch_moves;
mod batches;
mod children_joins;
mod children_rules;
mod claim_api;
mod claim_reads;
mod create_many;
mod create_retries;
mod create_rules;
mod fencing_retries;
mod governance_bounds;
mod governance_limits;
mod history_filters;
mod history_limits;
mod history_requests;
mod input_limits;
mod leases;
mod lineage;
mod list_order;
mod many_items;
mod metadata_shapes;
mod multi_state_claims;
mod named_values;
mod partition_scopes;
mod payload_refs;
mod plain_values;
mod policy_commands;
mod policy_lifecycle;
mod priority_retries;
mod priority_shards;
mod query_semantics;
mod read_api;
mod reclaim_batches;
mod records_and_claims;
mod retention_values;
mod schedule_api;
mod schedule_catchups;
mod schedule_fires;
mod schedule_lifecycle;
mod schedule_namespace;
mod schedule_overlaps;
mod schedule_recovery;
mod shared_values;
mod signal;
mod spawn_limits;
mod spawn_waits;
mod state_meta;
mod steps_contract;
mod terminal_batches;
mod transition_guards;
mod value_payload;
mod value_refs;
mod value_retention;
mod wait_any;
mod write_contract;

/// A reply from the node: the Elixir-shaped term or the refusal.
type Reply = Result<Term, ServiceError>;

/// A typed-map option body.
fn map(entries: &[(&str, Term)]) -> Term {
    Term::Map(
        entries
            .iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value.clone()))
            .collect(),
    )
}

fn text(value: &str) -> Term {
    Term::text(value)
}

const fn int(value: i64) -> Term {
    Term::Int(value)
}

/// A list of texts.
fn texts(values: &[&str]) -> Term {
    Term::List(values.iter().map(|value| text(value)).collect())
}

/// Runs a generic FLOW command; a refusal while reading it is a refusal
/// like any other, as the client sees it.
fn run(node: &QueueNode, name: &str, entries: &[(&str, Term)]) -> TestResult<Reply> {
    match flow::parse(name, &map(entries)) {
        Ok(command) => call(|reply| node.execute(command, reply)),
        Err(refused) => Ok(Err(refused)),
    }
}

/// Runs a generic FLOW command that must succeed.
fn ok(node: &QueueNode, name: &str, entries: &[(&str, Term)]) -> TestResult<Term> {
    Ok(run(node, name, entries)??)
}

/// The reply text of a refusal, or a test error when the command succeeded.
fn refusal(reply: Reply) -> TestResult<String> {
    match reply {
        Ok(term) => Err(format!("expected a refusal, got {term:?}").into()),
        Err(error) => Ok(error.to_string()),
    }
}

/// A field of a map reply, nil when absent.
fn field(term: &Term, key: &str) -> Term {
    term.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
}

/// `term` with every map's entries in key order, so maps compare as
/// Elixir's do, without order.
fn sorted(term: &Term) -> Term {
    match term {
        Term::Map(entries) => {
            let mut entries: Vec<(Vec<u8>, Term)> = entries
                .iter()
                .map(|(key, value)| (key.clone(), sorted(value)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Term::Map(entries)
        }
        Term::List(items) => Term::List(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// Whether a map reply has `key` at all.
fn has(term: &Term, key: &str) -> bool {
    term.get(key.as_bytes()).is_some()
}

/// A typed `FLOW.CLAIM_DUE` (or `FLOW.RECLAIM`) built as Elixir replies.
fn claim(node: &QueueNode, name: &str, entries: &[(&str, Term)]) -> TestResult<Reply> {
    let parsed = match claim::parse(name, &map(entries)) {
        Ok(parsed) => parsed,
        Err(refused) => return Ok(Err(refused)),
    };
    let shape = parsed.shape;
    let claimed = call(|reply| node.claim(parsed.request, reply))?;
    Ok(claimed.map(|claimed| claim::reply(shape, claimed)))
}

/// The claimed records of a claim that must succeed.
fn claimed(node: &QueueNode, entries: &[(&str, Term)]) -> TestResult<Vec<Term>> {
    match claim(node, "FLOW.CLAIM_DUE", entries)?? {
        Term::List(records) => Ok(records),
        other => Err(format!("claim reply is not a list: {other:?}").into()),
    }
}

/// `FLOW.GET` with extra options; nil when the Flow does not exist.
fn get(node: &QueueNode, id: &str, extra: &[(&str, Term)]) -> TestResult<Term> {
    let mut entries = vec![("id", text(id))];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.GET", &entries)
}

/// The field maps of a Flow's history events, oldest first.
fn events(node: &QueueNode, id: &str, extra: &[(&str, Term)]) -> TestResult<Vec<Term>> {
    let mut entries = vec![("id", text(id))];
    entries.extend(extra.iter().cloned());
    let Term::List(events) = ok(node, "FLOW.HISTORY", &entries)? else {
        return Err("history is not a list".into());
    };
    Ok(events
        .into_iter()
        .filter_map(|event| match event {
            Term::List(mut parts) if parts.len() == 2 => parts.pop(),
            _ => None,
        })
        .collect())
}
