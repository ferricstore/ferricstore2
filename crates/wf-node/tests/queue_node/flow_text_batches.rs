//! FLOW batch commands and claims as text, as the SDKs send them through
//! `COMMAND_EXEC` and Elixir's `NativeAstParser` reads them: a claim's
//! repeated `STATE` names several states (audit gap G55), and the batch
//! forms (`CREATE_MANY`, `COMPLETE_MANY`, `RETRY_MANY`, `FAIL_MANY`,
//! `CANCEL_MANY`, `TRANSITION_MANY`, `SPAWN_CHILDREN`) read their options up
//! to `ITEMS`, then fixed-width items, `MIXED` naming each item's partition
//! (audit gap G56). Found by the TypeScript SDK's live suite.

use wf_application::commands::flow::text::{self, Parsed};
use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::{QueueService, ServiceError};
use wf_node::composition::QueueNode;

use super::{TestResult, call, directory, open};

fn text(value: &str) -> Term {
    Term::text(value)
}

fn words(values: &[&str]) -> Vec<Term> {
    values.iter().map(|value| text(value)).collect()
}

/// Runs text command `name` with `args`.
fn exec(node: &QueueNode, name: &str, args: &[Term]) -> TestResult<Result<Term, ServiceError>> {
    match text::parse(name.as_bytes(), args) {
        Ok(Parsed::Command(command)) => Ok(call(|reply| node.execute(command, reply))?),
        Ok(Parsed::Claim(typed, map)) => {
            let parsed = flow::claim::parse(typed, &map)?;
            let shape = parsed.shape;
            let claimed = call(|reply| node.claim(parsed.request, reply))?;
            Ok(claimed.map(|claimed| flow::claim::reply(shape, claimed)))
        }
        Err(error) => Ok(Err(error)),
    }
}

fn ok(node: &QueueNode, name: &str, args: &[&str]) -> TestResult<Term> {
    Ok(exec(node, name, &words(args))??)
}

fn field(term: &Term, key: &str) -> Term {
    term.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
}

fn state_of(node: &QueueNode, id: &str, partition: &str) -> TestResult<Term> {
    let record = ok(node, "FLOW.GET", &[id, "PARTITION", partition])?;
    Ok(field(&record, "state"))
}

/// Claims every due Flow of `flow_type` in `partition`; ids and lease
/// options in claim order.
fn claim_all(
    node: &QueueNode,
    flow_type: &str,
    partition: &str,
) -> TestResult<Vec<(Term, Term, Term)>> {
    let claimed = ok(
        node,
        "FLOW.CLAIM_DUE",
        &[
            flow_type,
            "WORKER",
            "w",
            "LIMIT",
            "10",
            "NOW",
            "2000",
            "PARTITION",
            partition,
            "RETURN",
            "RECORDS",
        ],
    )?;
    let Term::List(records) = claimed else {
        return Err(format!("claim reply is not a list: {claimed:?}").into());
    };
    Ok(records
        .iter()
        .map(|record| {
            (
                field(record, "id"),
                field(record, "lease_token"),
                field(record, "fencing_token"),
            )
        })
        .collect())
}

fn word_of(term: &Term) -> String {
    match term {
        Term::Bin(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        Term::Int(value) => value.to_string(),
        other => format!("{other:?}"),
    }
}

#[test]
fn a_text_claim_takes_every_repeated_state() -> TestResult {
    // The TypeScript SDK's "claims full records across multiple states in
    // one command response": `STATE created STATE charged` claims both.
    let node = open(&directory("text-multi-state")?, 4)?;
    for state in ["created", "charged"] {
        ok(
            &node,
            "FLOW.CREATE",
            &[
                state,
                "TYPE",
                "multi",
                "STATE",
                state,
                "PARTITION",
                "p",
                "RUN_AT",
                "1000",
                "NOW",
                "1000",
            ],
        )?;
    }
    let claimed = ok(
        &node,
        "FLOW.CLAIM_DUE",
        &[
            "multi",
            "STATE",
            "created",
            "STATE",
            "charged",
            "WORKER",
            "w",
            "LIMIT",
            "2",
            "NOW",
            "1001",
            "PARTITION",
            "p",
            "RETURN",
            "RECORDS",
        ],
    )?;
    let Term::List(records) = claimed else {
        return Err("claim reply is not a list".into());
    };
    let mut ids: Vec<String> = records
        .iter()
        .map(|record| word_of(&field(record, "id")))
        .collect();
    ids.sort();
    assert_eq!(ids, ["charged", "created"]);
    Ok(())
}

#[test]
fn text_batches_create_complete_and_cancel_their_items() -> TestResult {
    // Elixir: "parses Flow many and newer workflow/governance command
    // names" and "Flow batch ACL extraction preserves mixed item
    // partitions"; the SDK's "completes acknowledged jobs" and "stores state
    // metadata for every flow mutation command".
    let node = open(&directory("text-batches")?, 4)?;
    let created = ok(
        &node,
        "FLOW.CREATE_MANY",
        &[
            "tenant", "TYPE", "batch", "STATE", "queued", "RUN_AT", "1000", "NOW", "1000", "ITEMS",
            "a", "pa", "b", "pb", "c", "pc",
        ],
    )?;
    assert_ne!(created, Term::Nil);
    let jobs = claim_all(&node, "batch", "tenant")?;
    assert_eq!(jobs.len(), 3);
    let (first, second) = (&jobs[0], &jobs[1]);
    let mut complete = vec![
        "tenant".to_owned(),
        "RESULT".to_owned(),
        "done".to_owned(),
        "NOW".to_owned(),
        "2001".to_owned(),
        "ITEMS".to_owned(),
    ];
    for job in [first, second] {
        complete.extend([word_of(&job.0), word_of(&job.1), word_of(&job.2)]);
    }
    let complete: Vec<&str> = complete.iter().map(String::as_str).collect();
    assert_eq!(ok(&node, "FLOW.COMPLETE_MANY", &complete)?, text("OK"));
    for job in [first, second] {
        assert_eq!(
            state_of(&node, &word_of(&job.0), "tenant")?,
            text("completed")
        );
    }
    // A cancel item names no lease, so it cancels a Flow nobody holds, as
    // Elixir's `flow_require_transition_lease/2` requires.
    ok(
        &node,
        "FLOW.CREATE",
        &[
            "waiting",
            "TYPE",
            "batch",
            "PARTITION",
            "tenant",
            "RUN_AT",
            "9000",
            "NOW",
            "1000",
        ],
    )?;
    assert_eq!(
        ok(
            &node,
            "FLOW.CANCEL_MANY",
            &["MIXED", "REASON", "stop", "ITEMS", "waiting", "tenant", "0"]
        )?,
        text("OK")
    );
    assert_eq!(state_of(&node, "waiting", "tenant")?, text("cancelled"));
    Ok(())
}

#[test]
fn text_transitions_retries_and_fails_reach_every_item() -> TestResult {
    let node = open(&directory("text-batch-moves")?, 4)?;
    ok(
        &node,
        "FLOW.CREATE_MANY",
        &[
            "moves", "TYPE", "moves", "RUN_AT", "1000", "NOW", "1000", "ITEMS", "m1", "x", "m2",
            "x", "m3", "x", "m4", "x",
        ],
    )?;
    let jobs = claim_all(&node, "moves", "moves")?;
    assert_eq!(jobs.len(), 4);
    let id = |index: usize| word_of(&jobs[index].0);
    let lease = |index: usize| word_of(&jobs[index].1);
    let fence = |index: usize| word_of(&jobs[index].2);
    let transition = [
        "moves".to_owned(),
        "running".to_owned(),
        "review".to_owned(),
        "NOW".to_owned(),
        "2001".to_owned(),
        "ITEMS".to_owned(),
        id(0),
        fence(0),
        lease(0),
    ];
    let transition: Vec<&str> = transition.iter().map(String::as_str).collect();
    ok(&node, "FLOW.TRANSITION_MANY", &transition)?;
    assert_eq!(state_of(&node, &id(0), "moves")?, text("review"));
    let retry = [
        "moves".to_owned(),
        "ERROR".to_owned(),
        "boom".to_owned(),
        "MAX_RETRIES".to_owned(),
        "0".to_owned(),
        "EXHAUSTED_TO".to_owned(),
        "dead".to_owned(),
        "NOW".to_owned(),
        "2001".to_owned(),
        "ITEMS".to_owned(),
        id(1),
        lease(1),
        fence(1),
    ];
    let retry: Vec<&str> = retry.iter().map(String::as_str).collect();
    // The retry's own limit (nested under `retry`) is spent at once.
    ok(&node, "FLOW.RETRY_MANY", &retry)?;
    assert_eq!(state_of(&node, &id(1), "moves")?, text("dead"));
    let fail = [
        "moves".to_owned(),
        "ERROR".to_owned(),
        "boom".to_owned(),
        "NOW".to_owned(),
        "2001".to_owned(),
        "ITEMS".to_owned(),
        id(2),
        lease(2),
        fence(2),
    ];
    let fail: Vec<&str> = fail.iter().map(String::as_str).collect();
    ok(&node, "FLOW.FAIL_MANY", &fail)?;
    assert_eq!(state_of(&node, &id(2), "moves")?, text("failed"));
    Ok(())
}

#[test]
fn text_spawn_children_and_item_grammar() -> TestResult {
    // Elixir: "Flow item commands read an option value spelled ITEMS as a
    // value" and "malformed Flow item commands return the parser's error".
    let node = open(&directory("text-batch-spawn")?, 4)?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            "parent",
            "TYPE",
            "parent",
            "PARTITION",
            "fam",
            "RUN_AT",
            "1000",
            "NOW",
            "1000",
        ],
    )?;
    let jobs = claim_all(&node, "parent", "fam")?;
    let (_, lease, fence) = jobs.first().ok_or("parent not claimed")?;
    let spawn = [
        "parent".to_owned(),
        "PARTITION".to_owned(),
        "fam".to_owned(),
        "GROUP".to_owned(),
        "items".to_owned(),
        "SUCCESS".to_owned(),
        "done".to_owned(),
        "FAILURE".to_owned(),
        "failed".to_owned(),
        "WAIT_STATE".to_owned(),
        "waiting_children".to_owned(),
        "LEASE_TOKEN".to_owned(),
        word_of(lease),
        "FENCING".to_owned(),
        word_of(fence),
        "NOW".to_owned(),
        "2001".to_owned(),
        "ITEMS".to_owned(),
        "child-1".to_owned(),
        "child".to_owned(),
        "payload".to_owned(),
    ];
    let spawn: Vec<&str> = spawn.iter().map(String::as_str).collect();
    ok(&node, "FLOW.SPAWN_CHILDREN", &spawn)?;
    let child = ok(&node, "FLOW.GET", &["child-1", "PARTITION", "fam"])?;
    assert_eq!(field(&child, "type"), text("child"));
    let attributed = ok(
        &node,
        "FLOW.CREATE_MANY",
        &[
            "tenant",
            "TYPE",
            "items",
            "ATTRIBUTE",
            "source",
            "ITEMS",
            "ITEMS",
            "flow-a",
            "payload-a",
        ],
    )?;
    assert_ne!(attributed, Term::Nil);
    let made = ok(&node, "FLOW.GET", &["flow-a", "PARTITION", "tenant"])?;
    assert_eq!(field(&made, "type"), text("items"));
    Ok(())
}

#[test]
fn malformed_text_batches_return_the_parser_s_error() -> TestResult {
    // Elixir: "malformed Flow item commands return the parser's error".
    let node = open(&directory("text-batch-malformed")?, 4)?;
    for (name, args) in [
        (
            "FLOW.SPAWN_CHILDREN",
            vec![
                "parent", "GROUP", "g", "BOGUS", "x", "ITEMS", "child", "type", "payload",
            ],
        ),
        (
            "FLOW.SPAWN_CHILDREN",
            vec!["parent", "GROUP", "g", "ITEMS", "child", "type"],
        ),
        (
            "FLOW.CREATE_MANY",
            vec!["tenant", "TYPE", "t", "ITEMS", "flow-a", "payload-a", "x"],
        ),
        (
            "FLOW.COMPLETE_MANY",
            vec!["tenant", "BOGUS", "x", "ITEMS", "flow-a", "lease", "1"],
        ),
        ("FLOW.CANCEL_MANY", vec!["tenant", "ITEMS", "flow-a"]),
        (
            "FLOW.TRANSITION_MANY",
            vec!["tenant", "a", "b", "ITEMS", "flow-a", "lease"],
        ),
        ("FLOW.CANCEL_MANY", vec!["tenant", "REASON", "x"]),
    ] {
        let refused = exec(&node, name, &words(&args))?;
        let text = refused
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(text.starts_with("ERR "), "{name} {args:?}: {text}");
        assert!(!text.contains("not supported"), "{name} {args:?}: {text}");
    }
    let empty = exec(
        &node,
        "FLOW.CANCEL_MANY",
        &words(&["tenant", "REASON", "x"]),
    )?;
    assert_eq!(
        empty.err().map(|error| error.to_string()),
        Some("ERR flow items are required".to_owned())
    );
    Ok(())
}
