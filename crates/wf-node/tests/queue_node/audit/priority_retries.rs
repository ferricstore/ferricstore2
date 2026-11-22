//! `flow_test/sections/flow_claim_due_drains_higher_priorities_before_
//! lower_priorities_default.exs`: claims take higher priorities first or
//! one named band; completions, retries and terminal Flows check their
//! leases and states; Flows due far ahead are claimed when due, by
//! partition, partition list, auto bucket or a blocking claim; retry
//! policies back off, exhaust to an active or terminal state, settle a
//! parent's group and refuse invalid limits. (Its LMDB parking, cold due
//! rows and claim waiter schedules are Elixir's.)

use wf_application::commands::flow::Term;
use wf_application::commands::queue::Partition;
use wf_node::composition::QueueNode;

use super::children_rules::{
    P as FAMILY, child, claim_child, create as create_in_family, path, spawn,
};
use super::{claimed, events, field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

/// Creates `id` of `flow_type`; `extra` adds or replaces options.
pub(super) fn create(
    node: &QueueNode,
    id: &str,
    flow_type: &str,
    extra: &[(&str, Term)],
) -> TestResult {
    let mut entries = vec![
        ("id", text(id)),
        ("type", text(flow_type)),
        ("run_at_ms", int(1_000)),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

/// Claims `flow_type` at `now`; `extra` adds or replaces options.
pub(super) fn claim(
    node: &QueueNode,
    flow_type: &str,
    now: i64,
    extra: &[(&str, Term)],
) -> TestResult<Vec<Term>> {
    let mut entries = vec![
        ("type", text(flow_type)),
        ("worker", text("worker-a")),
        ("lease_ms", int(30_000)),
        ("limit", int(1)),
        ("now_ms", int(now)),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    claimed(node, &entries)
}

pub(super) fn one(records: Vec<Term>) -> TestResult<Term> {
    if records.len() != 1 {
        return Err(format!("one claim expected, got {records:?}").into());
    }
    records
        .into_iter()
        .next()
        .ok_or_else(|| "one claim expected".into())
}

fn fence(lease: &Term, raise: i64) -> TestResult<Term> {
    let Term::Int(fence) = field(lease, "fencing_token") else {
        return Err("no fence".into());
    };
    Ok(int(fence.saturating_add(raise)))
}

/// Runs `command` on the claimed `lease` with `token` and `extra`.
fn on_lease(
    node: &QueueNode,
    command: &str,
    lease: &Term,
    token: Term,
    extra: &[(&str, Term)],
) -> TestResult<Result<Term, String>> {
    let mut entries = vec![("id", field(lease, "id")), ("lease_token", token)];
    entries.extend(extra.iter().cloned());
    Ok(run(node, command, &entries)?.map_err(|error| error.to_string()))
}

#[test]
fn claims_take_higher_priorities_first_or_one_named_band() -> TestResult {
    // Elixir: "flow_claim_due drains higher priorities before lower
    // priorities by default" and "flow_claim_due priority option targets
    // one priority band".
    let node = open(&directory("audit-priority-bands")?, 4)?;
    for flow_type in ["priority", "priority-target"] {
        create(
            &node,
            &format!("{flow_type}-low"),
            flow_type,
            &[("priority", int(0))],
        )?;
        create(
            &node,
            &format!("{flow_type}-high"),
            flow_type,
            &[("priority", int(2))],
        )?;
    }
    let first = one(claim(&node, "priority", 1_000, &[])?)?;
    assert_eq!(field(&first, "id"), text("priority-high"));
    let second = one(claim(&node, "priority", 1_000, &[])?)?;
    assert_eq!(field(&second, "id"), text("priority-low"));
    let low = one(claim(
        &node,
        "priority-target",
        1_000,
        &[("priority", int(0))],
    )?)?;
    assert_eq!(field(&low, "id"), text("priority-target-low"));
    let high = one(claim(
        &node,
        "priority-target",
        1_000,
        &[("priority", int(2))],
    )?)?;
    assert_eq!(field(&high, "id"), text("priority-target-high"));
    Ok(())
}

#[test]
fn a_completion_checks_its_lease_and_finishes_the_flow() -> TestResult {
    // Elixir: "flow_complete enforces lease token guard and writes terminal
    // state".
    let node = open(&directory("audit-priority-complete")?, 4)?;
    create(&node, "done", "image", &[("state", text("queued"))])?;
    let lease = one(claim(&node, "image", 1_000, &[("state", text("queued"))])?)?;
    let result = ("result", text("result:done"));
    let stale = Err("ERR stale flow lease".to_owned());
    let wrong = [("fencing_token", fence(&lease, 0)?), result.clone()];
    assert_eq!(
        on_lease(&node, "FLOW.COMPLETE", &lease, text("wrong-token"), &wrong)?,
        stale
    );
    let token = field(&lease, "lease_token");
    let raised = [("fencing_token", fence(&lease, 1)?), result.clone()];
    assert_eq!(
        on_lease(&node, "FLOW.COMPLETE", &lease, token.clone(), &raised)?,
        stale
    );
    let right = [("fencing_token", fence(&lease, 0)?), result];
    on_lease(&node, "FLOW.COMPLETE", &lease, token, &right)??;
    let record = get(&node, "done", &[])?;
    assert_eq!(field(&record, "state"), text("completed"));
    assert!(matches!(field(&record, "result_ref"), Term::Bin(_)));
    assert_ne!(field(&record, "result_ref"), text("result:done"));
    assert_eq!(field(&record, "lease_token"), Term::Nil);
    assert_eq!(field(&record, "version"), int(3));
    Ok(())
}

#[test]
fn a_finished_flow_refuses_transitions_and_cancels() -> TestResult {
    // Elixir: "terminal flows reject normal transition and cancel".
    let node = open(&directory("audit-priority-terminal")?, 4)?;
    create(&node, "finished", "image", &[("state", text("queued"))])?;
    let lease = one(claim(&node, "image", 1_000, &[("state", text("queued"))])?)?;
    let right = [("fencing_token", fence(&lease, 0)?)];
    on_lease(
        &node,
        "FLOW.COMPLETE",
        &lease,
        field(&lease, "lease_token"),
        &right,
    )??;
    let finished = get(&node, "finished", &[])?;
    let terminal = "ERR flow is terminal; use FLOW.REWIND";
    let moved = run(
        &node,
        "FLOW.TRANSITION",
        &[
            ("id", text("finished")),
            ("from_state", text("completed")),
            ("to_state", text("queued")),
            ("fencing_token", field(&finished, "fencing_token")),
        ],
    )?;
    assert_eq!(refusal(moved)?, terminal);
    let cancelled = run(
        &node,
        "FLOW.CANCEL",
        &[
            ("id", text("finished")),
            ("fencing_token", field(&finished, "fencing_token")),
        ],
    )?;
    assert_eq!(refusal(cancelled)?, terminal);
    assert_eq!(
        field(&get(&node, "finished", &[])?, "state"),
        text("completed")
    );
    Ok(())
}

#[test]
fn a_retry_clears_the_lease_and_reschedules() -> TestResult {
    // Elixir: "flow_retry clears lease and reschedules flow".
    let node = open(&directory("audit-priority-retry")?, 4)?;
    create(&node, "hook", "webhook", &[("state", text("queued"))])?;
    let lease = one(claim(
        &node,
        "webhook",
        1_000,
        &[("state", text("queued"))],
    )?)?;
    let token = field(&lease, "lease_token");
    let options = |raise| -> TestResult<Vec<(&str, Term)>> {
        Ok(vec![
            ("fencing_token", fence(&lease, raise)?),
            ("error", text("error:hook")),
            ("run_at_ms", int(2_000)),
        ])
    };
    let stale = on_lease(&node, "FLOW.RETRY", &lease, token.clone(), &options(1)?)?;
    assert_eq!(stale, Err("ERR stale flow lease".to_owned()));
    on_lease(&node, "FLOW.RETRY", &lease, token, &options(0)?)??;
    let record = get(&node, "hook", &[])?;
    assert_eq!(field(&record, "state"), text("queued"));
    assert_eq!(field(&record, "attempts"), int(1));
    assert!(matches!(field(&record, "error_ref"), Term::Bin(_)));
    assert_ne!(field(&record, "error_ref"), text("error:hook"));
    assert_eq!(field(&record, "lease_token"), Term::Nil);
    let again = one(claim(
        &node,
        "webhook",
        2_000,
        &[("state", text("queued")), ("worker", text("worker-b"))],
    )?)?;
    assert_eq!(
        (field(&again, "id"), field(&again, "lease_owner")),
        (text("hook"), text("worker-b"))
    );
    Ok(())
}

/// The auto partition a Flow without one gets, as the SDK computes it.
fn auto(id: &str) -> Term {
    Term::Bin(Partition::for_new_flow(id.as_bytes(), None).name())
}

#[test]
fn a_flow_due_far_ahead_is_claimed_when_due() -> TestResult {
    // Elixir: "flow_claim_due promotes hibernated cold due flow", its single
    // Flows (Elixir parks them in LMDB; the node keeps them indexed).
    let node = open(&directory("audit-priority-far")?, 4)?;
    let kind = "cold-due:bench";
    let waiting = |id: &str, now: i64, due: i64, partition: Option<&str>| {
        let mut extra = vec![
            ("state", text("waiting")),
            ("now_ms", int(now)),
            ("run_at_ms", int(due)),
        ];
        extra.extend(partition.map(|partition| ("partition_key", text(partition))));
        create(&node, id, kind, &extra)
    };
    waiting("far", 0, 301_000, Some("far-p"))?;
    let in_far = [("partition_key", text("far-p"))];
    assert_eq!(
        field(&get(&node, "far", &in_far)?, "next_run_at_ms"),
        int(301_000)
    );
    let named = |worker: &str| {
        vec![
            ("state", text("waiting")),
            ("partition_key", text("far-p")),
            ("worker", text(worker)),
        ]
    };
    assert_eq!(
        claim(&node, kind, 300_999, &named("before"))?,
        Vec::<Term>::new()
    );
    let due = one(claim(&node, kind, 301_000, &named("after"))?)?;
    assert_eq!(
        (field(&due, "id"), field(&due, "lease_owner")),
        (text("far"), text("after"))
    );

    let listed = |partition: Term, extra: Option<(&'static str, Term)>| {
        let mut options = vec![
            ("state", text("any")),
            ("partition_keys", Term::List(vec![partition])),
            ("worker", text("listed")),
        ];
        options.extend(extra);
        options
    };
    waiting("listed", 10_000, 312_000, Some("far-p"))?;
    let taken = one(claim(&node, kind, 312_000, &listed(text("far-p"), None))?)?;
    assert_eq!(field(&taken, "id"), text("listed"));
    waiting("auto", 20_000, 322_000, None)?;
    assert_eq!(
        field(&get(&node, "auto", &[])?, "partition_key"),
        auto("auto")
    );
    let taken = one(claim(&node, kind, 322_000, &listed(auto("auto"), None))?)?;
    assert_eq!(field(&taken, "id"), text("auto"));
    waiting("blocking", 30_000, 332_000, None)?;
    let blocking = listed(auto("blocking"), Some(("block_ms", int(1))));
    let taken = one(claim(&node, kind, 332_000, &blocking)?)?;
    assert_eq!(field(&taken, "id"), text("blocking"));

    let epoch: i64 = 1_780_318_697_811;
    waiting("epoch", epoch, epoch.saturating_add(301_000), None)?;
    let taken = one(claim(
        &node,
        kind,
        epoch.saturating_add(301_000),
        &listed(auto("epoch"), None),
    )?)?;
    assert_eq!(field(&taken, "id"), text("epoch"));
    Ok(())
}

/// Claims `flow_type` in `partitions` over up to eight rounds; the ids.
fn claim_rounds(
    node: &QueueNode,
    flow_type: &str,
    partitions: &[Term],
    at: i64,
) -> TestResult<Vec<Term>> {
    let mut taken: Vec<Term> = Vec::new();
    for round in 1..=8 {
        let records = claim(
            node,
            flow_type,
            at.saturating_add(round),
            &[
                ("state", text("any")),
                ("partition_keys", Term::List(partitions.to_vec())),
                ("limit", int(64)),
            ],
        )?;
        taken.extend(records.iter().map(|record| field(record, "id")));
    }
    taken.sort_by_key(|id| match id {
        Term::Bin(bytes) => bytes.clone(),
        _ => Vec::new(),
    });
    Ok(taken)
}

#[test]
fn flows_due_far_ahead_in_auto_buckets_are_claimed_by_their_keys() -> TestResult {
    // Elixir: "flow_claim_due promotes hibernated cold due flow", its
    // batches: an independent batch without partitions, and one batch per
    // auto bucket as the SDK groups them.
    let node = open(&directory("audit-priority-far-batches")?, 4)?;
    let names = |prefix: &str| -> Vec<String> {
        (1..=64)
            .map(|index| format!("{prefix}-{index:02}"))
            .collect()
    };
    let buckets = |ids: &[String]| {
        let mut partitions: Vec<Term> = ids.iter().map(|id| auto(id)).collect();
        partitions.sort_by_key(|partition| format!("{partition:?}"));
        partitions.dedup();
        partitions
    };
    let batch = |flow_type: &str, partition: Option<Term>, ids: &[String], now: i64| {
        let mut entries = vec![
            ("type", text(flow_type)),
            ("state", text("waiting")),
            ("now_ms", int(now)),
            ("run_at_ms", int(now.saturating_add(302_000))),
            ("independent", Term::Bool(true)),
            ("items", Term::List(ids.iter().map(|id| text(id)).collect())),
        ];
        entries.extend(partition.map(|partition| ("partition_key", partition)));
        ok(&node, "FLOW.CREATE_MANY", &entries)
    };
    let many = names("many");
    batch("many:bench", None, &many, 40_000)?;
    let expected: Vec<Term> = many.iter().map(|id| text(id)).collect();
    assert_eq!(
        claim_rounds(&node, "many:bench", &buckets(&many), 342_000)?,
        expected
    );

    let grouped = names("grouped");
    for bucket in buckets(&grouped) {
        let ids: Vec<String> = grouped
            .iter()
            .filter(|id| auto(id) == bucket)
            .cloned()
            .collect();
        batch("grouped:bench", Some(bucket), &ids, 50_000)?;
    }
    let expected: Vec<Term> = grouped.iter().map(|id| text(id)).collect();
    assert_eq!(
        claim_rounds(&node, "grouped:bench", &buckets(&grouped), 352_000)?,
        expected
    );
    Ok(())
}

fn retry_with(node: &QueueNode, lease: &Term, policy: Term) -> TestResult<Result<Term, String>> {
    on_lease(
        node,
        "FLOW.RETRY",
        lease,
        field(lease, "lease_token"),
        &[
            ("fencing_token", fence(lease, 0)?),
            ("now_ms", int(2_000)),
            ("retry", policy),
        ],
    )
}

fn backoff(kind: &str, base_ms: i64, max_ms: i64) -> Term {
    map(&[
        ("kind", text(kind)),
        ("base_ms", int(base_ms)),
        ("max_ms", int(max_ms)),
        ("jitter_pct", int(0)),
    ])
}

/// The fields of `id`'s `retry` event.
fn retry_event(node: &QueueNode, id: &str) -> TestResult<Term> {
    events(node, id, &[])?
        .into_iter()
        .find(|fields| field(fields, "event") == text("retry"))
        .ok_or_else(|| "no retry event".into())
}

#[test]
fn a_retry_policy_backs_off_into_the_claimed_state() -> TestResult {
    // Elixir: "flow_retry returns to claimed run_state with computed
    // backoff".
    let node = open(&directory("audit-priority-backoff")?, 4)?;
    create(&node, "pay", "payment", &[("state", text("charge_card"))])?;
    let state = [("state", text("charge_card"))];
    let lease = one(claim(&node, "payment", 1_000, &state)?)?;
    assert_eq!(field(&lease, "state"), text("running"));
    assert_eq!(field(&lease, "run_state"), text("charge_card"));
    let policy = map(&[
        ("max_retries", int(3)),
        ("backoff", backoff("exponential", 1_000, 30_000)),
        ("exhausted_to", text("payment_failed")),
    ]);
    retry_with(&node, &lease, policy)??;
    let record = get(&node, "pay", &[])?;
    assert_eq!(field(&record, "state"), text("charge_card"));
    assert_eq!(field(&record, "attempts"), int(1));
    assert_eq!(field(&record, "next_run_at_ms"), int(3_000));
    assert_eq!(field(&record, "lease_token"), Term::Nil);
    let retry = retry_event(&node, "pay")?;
    for (name, value) in [
        ("retry_decision", "scheduled"),
        ("retry_run_state", "charge_card"),
        ("retry_next_run_at_ms", "3000"),
        ("retry_max_retries", "3"),
        ("retry_backoff_kind", "exponential"),
        ("retry_backoff_base_ms", "1000"),
        ("retry_backoff_max_ms", "30000"),
        ("retry_jitter_pct", "0"),
        ("retry_exhausted_to", "payment_failed"),
    ] {
        assert_eq!(field(&retry, name), text(value), "{name}");
    }
    assert_eq!(claim(&node, "payment", 2_999, &state)?, Vec::<Term>::new());
    let again = one(claim(&node, "payment", 3_000, &state)?)?;
    assert_eq!(
        (field(&again, "id"), field(&again, "run_state")),
        (text("pay"), text("charge_card"))
    );
    Ok(())
}

#[test]
fn a_retry_that_exhausts_moves_to_its_active_or_terminal_state() -> TestResult {
    // Elixir: "flow_retry exhausts to configured active state" and
    // "flow_retry terminal exhaustion keeps stable audit metadata".
    let node = open(&directory("audit-priority-exhaust")?, 4)?;
    let state = [("state", text("charge_card"))];
    for (id, flow_type, to) in [
        ("active", "payment-exhaust", "payment_failed"),
        ("terminal", "payment-terminal-exhaust", "failed"),
    ] {
        create(&node, id, flow_type, &state)?;
        let lease = one(claim(&node, flow_type, 1_000, &state)?)?;
        let policy = map(&[
            ("max_retries", int(0)),
            ("backoff", backoff("fixed", 10_000, 10_000)),
            ("exhausted_to", text(to)),
        ]);
        retry_with(&node, &lease, policy)??;
    }
    let active = get(&node, "active", &[])?;
    assert_eq!(field(&active, "state"), text("payment_failed"));
    assert_eq!(field(&active, "attempts"), int(1));
    assert_eq!(field(&active, "next_run_at_ms"), int(2_000));
    assert_eq!(field(&active, "lease_token"), Term::Nil);
    let manual = one(claim(
        &node,
        "payment-exhaust",
        2_000,
        &[("state", text("payment_failed"))],
    )?)?;
    assert_eq!(field(&manual, "id"), text("active"));

    let terminal = get(&node, "terminal", &[])?;
    assert_eq!(field(&terminal, "state"), text("failed"));
    assert_eq!(field(&terminal, "next_run_at_ms"), Term::Nil);
    let retry = retry_event(&node, "terminal")?;
    assert_eq!(field(&retry, "retry_decision"), text("exhausted"));
    assert_eq!(field(&retry, "retry_next_run_at_ms"), text(""));
    assert_eq!(field(&retry, "retry_exhausted_to"), text("failed"));
    Ok(())
}

#[test]
fn a_retry_that_exhausts_settles_the_parent_group() -> TestResult {
    // Elixir: "flow_retry terminal exhaustion updates colocated parent child
    // group".
    let node = open(&directory("audit-priority-family")?, 4)?;
    let parent = create_in_family(&node, "parent", "parent", "dispatch", 1_000)?;
    spawn(
        &node,
        "parent",
        vec![child("kid")],
        &[
            ("group_id", text("retry-fanout")),
            ("on_parent_closed", text("abandon_children")),
            ("fencing_token", field(&parent, "fencing_token")),
            (
                "exhaust_to",
                map(&[
                    ("success", text("children_done")),
                    ("failure", text("children_failed")),
                ]),
            ),
        ],
    )??;
    let lease = claim_child(&node, "kid", "worker-retry-cross")?;
    let in_family = ("partition_key", text(FAMILY));
    on_lease(
        &node,
        "FLOW.RETRY",
        &lease,
        field(&lease, "lease_token"),
        &[
            in_family.clone(),
            ("fencing_token", fence(&lease, 0)?),
            ("now_ms", int(9_000_000_000_001)),
            (
                "retry",
                map(&[("max_retries", int(0)), ("exhausted_to", text("failed"))]),
            ),
        ],
    )??;
    assert_eq!(
        field(
            &get(&node, "kid", std::slice::from_ref(&in_family))?,
            "state"
        ),
        text("failed")
    );
    let parent = get(&node, "parent", &[in_family])?;
    assert_eq!(field(&parent, "state"), text("children_failed"));
    let group = |rest: &[&str]| {
        let mut keys = vec!["child_groups", "retry-fanout"];
        keys.extend_from_slice(rest);
        path(&parent, &keys)
    };
    assert_eq!(group(&["children", "kid"]), text("failed"));
    assert_eq!(group(&["summary", "failed"]), int(1));
    Ok(())
}

#[test]
fn a_retry_refuses_an_invalid_policy() -> TestResult {
    // Elixir: "flow_retry rejects invalid retry policy".
    let node = open(&directory("audit-priority-invalid")?, 4)?;
    create(&node, "invalid", "retry-policy-invalid", &[])?;
    let lease = one(claim(&node, "retry-policy-invalid", 1_000, &[])?)?;
    for (policy, refused) in [
        (
            map(&[("max_retries", int(1_001))]),
            "ERR flow retry max_retries must be between 0 and 1000",
        ),
        (
            map(&[("exhausted_to", text("running"))]),
            "ERR flow retry exhausted_to cannot be running",
        ),
    ] {
        assert_eq!(retry_with(&node, &lease, policy)?, Err(refused.to_owned()));
    }
    Ok(())
}
