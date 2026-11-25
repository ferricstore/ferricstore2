//! `flow/invariant_test.exs` (rest-of-suite audit,
//! `docs/reviews/workflow-flow-test-audit-rest.md`): a FIFO state lets one
//! Flow of each type, state and partition run at a time, in state-entry
//! order.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

pub(super) const KIND: &str = "fifo-invariant";

pub(super) fn node(name: &str) -> TestResult<QueueNode> {
    open(&directory(name)?, 4)
}

/// Makes `states` of the test type FIFO (with `extra` policy fields).
pub(super) fn fifo(node: &QueueNode, states: &[&str], extra: &[(&str, Term)]) -> TestResult {
    let modes: Vec<(&str, Term)> = states
        .iter()
        .map(|state| (*state, map(&[("mode", text("fifo"))])))
        .collect();
    let mut entries = vec![("type", text(KIND)), ("states", map(&modes))];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.POLICY.SET", &entries)?;
    Ok(())
}

/// Creates a `queued` Flow of the test type in `partition`.
pub(super) fn create(
    node: &QueueNode,
    id: &str,
    partition: &str,
    now: i64,
    due: i64,
) -> TestResult {
    ok(
        node,
        "FLOW.CREATE",
        &[
            ("id", text(id)),
            ("type", text(KIND)),
            ("state", text("queued")),
            ("partition_key", text(partition)),
            ("now_ms", int(now)),
            ("run_at_ms", int(due)),
        ],
    )?;
    Ok(())
}

/// Claims due Flows of the test type at `now`, scoped by `scope`.
pub(super) fn claim(
    node: &QueueNode,
    scope: &[(&str, Term)],
    now: i64,
    worker: &str,
) -> TestResult<Vec<Term>> {
    let given = |name: &str| scope.iter().any(|(key, _)| *key == name);
    let mut entries = vec![
        ("type", text(KIND)),
        ("worker", text(worker)),
        ("now_ms", int(now)),
    ];
    if !given("limit") {
        entries.push(("limit", int(10)));
    }
    if !given("state") && !given("states") {
        entries.push(("state", text("queued")));
    }
    entries.extend(scope.iter().cloned());
    claimed(node, &entries)
}

/// The ids of `records`, sorted.
pub(super) fn ids(records: &[Term]) -> Vec<Term> {
    let mut ids: Vec<Term> = records.iter().map(|record| field(record, "id")).collect();
    ids.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    ids
}

pub(super) fn texts(values: &[&str]) -> Vec<Term> {
    let mut ids: Vec<Term> = values.iter().map(|value| text(value)).collect();
    ids.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    ids
}

pub(super) fn partition(name: &str) -> [(&'static str, Term); 1] {
    [("partition_key", text(name))]
}

/// Completes a claimed record.
pub(super) fn complete(node: &QueueNode, record: &Term, now: i64) -> TestResult {
    ok(
        node,
        "FLOW.COMPLETE",
        &[
            ("id", field(record, "id")),
            ("partition_key", field(record, "partition_key")),
            ("lease_token", field(record, "lease_token")),
            ("fencing_token", field(record, "fencing_token")),
            ("now_ms", int(now)),
        ],
    )?;
    Ok(())
}

#[test]
fn states_are_parallel_until_a_policy_makes_one_fifo() -> TestResult {
    // Elixir: "states are parallel by default until policy opts a state
    // into fifo" and "fifo state claims one flow per partition lane in
    // state-entry order" (ids sort against entry order).
    let node = node("audit-rest-fifo-parallel")?;
    create(&node, "z-par", "p", 1_000, 2_000)?;
    create(&node, "a-par", "p", 1_001, 2_000)?;
    let both = claim(&node, &partition("p"), 2_000, "w")?;
    assert_eq!(ids(&both), texts(&["a-par", "z-par"]));
    fifo(&node, &["queued"], &[])?;
    create(&node, "z-first", "q", 1_000, 2_000)?;
    create(&node, "a-second", "q", 1_001, 2_000)?;
    let first = claim(&node, &partition("q"), 2_000, "w")?;
    assert_eq!(ids(&first), texts(&["z-first"]));
    assert_eq!(claim(&node, &partition("q"), 2_001, "w")?, []);
    complete(&node, first.first().ok_or("one claimed")?, 2_002)?;
    let second = claim(&node, &partition("q"), 2_003, "w")?;
    assert_eq!(ids(&second), texts(&["a-second"]));
    Ok(())
}

#[test]
fn a_retried_head_keeps_its_lane_until_it_leaves() -> TestResult {
    // Elixir: "fifo retry keeps the same partition lane blocked until the
    // retried flow leaves".
    let node = node("audit-rest-fifo-retry")?;
    let backoff = map(&[
        ("kind", text("fixed")),
        ("base_ms", int(100)),
        ("max_ms", int(100)),
    ]);
    let retry = map(&[("max_retries", int(3)), ("backoff", backoff)]);
    fifo(&node, &["queued"], &[("retry", retry)])?;
    create(&node, "z-first", "p", 1_000, 2_000)?;
    create(&node, "a-second", "p", 1_001, 2_000)?;
    let first = claim(&node, &partition("p"), 2_000, "w")?;
    let head = first.first().ok_or("one claimed")?;
    assert_eq!(field(head, "id"), text("z-first"));
    ok(
        &node,
        "FLOW.RETRY",
        &[
            ("id", text("z-first")),
            ("partition_key", text("p")),
            ("lease_token", field(head, "lease_token")),
            ("fencing_token", field(head, "fencing_token")),
            ("error", text("try again")),
            ("now_ms", int(2_010)),
            ("run_at_ms", int(2_100)),
        ],
    )?;
    assert_eq!(claim(&node, &partition("p"), 2_050, "w")?, []);
    let retried = claim(&node, &partition("p"), 2_100, "w")?;
    assert_eq!(ids(&retried), texts(&["z-first"]));
    complete(&node, retried.first().ok_or("one claimed")?, 2_200)?;
    assert_eq!(
        ids(&claim(&node, &partition("p"), 2_201, "w")?),
        texts(&["a-second"])
    );
    Ok(())
}

#[test]
fn lanes_are_per_state_and_partition() -> TestResult {
    // Elixir: "fifo lanes are scoped to type state and partition", "fifo
    // any-partition claims at most one flow per partition lane" and "fifo
    // partition list claims at most one flow per listed partition lane".
    let node = node("audit-rest-fifo-scopes")?;
    fifo(&node, &["queued", "charge"], &[])?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("charge-1")),
            ("type", text(KIND)),
            ("state", text("charge")),
            ("partition_key", text("a")),
            ("now_ms", int(999)),
            ("run_at_ms", int(1_000)),
        ],
    )?;
    for (id, part, now) in [
        ("z-a1", "a", 1_000),
        ("a-a2", "a", 1_001),
        ("z-b1", "b", 1_002),
        ("a-b2", "b", 1_003),
        ("z-c1", "c", 1_004),
    ] {
        create(&node, id, part, now, 2_000)?;
    }
    let listed = Term::List(vec![text("a"), text("b")]);
    let heads = claim(&node, &[("partition_keys", listed.clone())], 2_000, "w")?;
    assert_eq!(ids(&heads), texts(&["z-a1", "z-b1"]));
    assert_eq!(claim(&node, &[("partition_keys", listed)], 2_001, "w")?, []);
    // The charge state's lane in `a` is its own.
    let charge = claim(
        &node,
        &[("state", text("charge")), ("partition_key", text("a"))],
        2_001,
        "w",
    )?;
    assert_eq!(ids(&charge), texts(&["charge-1"]));
    for head in &heads {
        complete(&node, head, 2_100)?;
    }
    let any = claim(&node, &[("partition_key", text("ANY"))], 2_101, "w")?;
    assert_eq!(ids(&any), texts(&["a-a2", "a-b2", "z-c1"]));
    assert_eq!(
        claim(&node, &[("partition_key", text("ANY"))], 2_102, "w")?,
        []
    );
    Ok(())
}

#[test]
fn an_expired_head_is_reclaimed_before_its_follower() -> TestResult {
    // Elixir: "fifo reclaim picks the expired lane head instead of the
    // queued follower" and "fifo limit one reclaims an expired lane head
    // before its queued follower".
    for limit in [1, 10] {
        let node = node(&format!("audit-rest-fifo-reclaim-{limit}"))?;
        fifo(&node, &["queued"], &[])?;
        create(&node, "z-first", "p", 1_000, 2_000)?;
        create(&node, "a-second", "p", 1_001, 2_000)?;
        let leased = [
            ("partition_key", text("p")),
            ("lease_ms", int(50)),
            ("limit", int(1)),
        ];
        assert_eq!(
            ids(&claim(&node, &leased, 2_000, "old")?),
            texts(&["z-first"])
        );
        assert_eq!(claim(&node, &partition("p"), 2_010, "w")?, []);
        let again = [
            ("partition_key", text("p")),
            ("lease_ms", int(50)),
            ("limit", int(limit)),
        ];
        let reclaimed = claim(&node, &again, 2_100, "new")?;
        assert_eq!(ids(&reclaimed), texts(&["z-first"]), "limit {limit}");
        let head = reclaimed.first().ok_or("one claimed")?;
        assert_eq!(field(head, "lease_owner"), text("new"));
        assert_eq!(claim(&node, &partition("p"), 2_101, "w")?, []);
    }
    Ok(())
}

#[test]
fn a_head_not_yet_due_holds_back_a_due_follower() -> TestResult {
    // Elixir: "fifo keeps a cold lane head ahead of a hot due follower".
    let node = node("audit-rest-fifo-head-not-due")?;
    fifo(&node, &["queued"], &[])?;
    create(&node, "z-head", "p", 1_000, 1_000_000)?;
    create(&node, "a-follower", "p", 1_001, 2_000)?;
    assert_eq!(claim(&node, &partition("p"), 2_000, "w")?, []);
    Ok(())
}

#[test]
fn an_overdue_follower_times_out_while_its_head_holds_the_lane() -> TestResult {
    // Elixir: "fifo times out an expired follower while a live lane head
    // blocks claims".
    let node = node("audit-rest-fifo-follower-timeout")?;
    fifo(&node, &["queued"], &[])?;
    create(&node, "head", "p", 500, 3_000)?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("follower")),
            ("type", text(KIND)),
            ("state", text("queued")),
            ("partition_key", text("p")),
            ("max_active_ms", int(100)),
            ("now_ms", int(600)),
            ("run_at_ms", int(1_000)),
        ],
    )?;
    assert_eq!(claim(&node, &partition("p"), 2_000, "w")?, []);
    let head = get(&node, "head", &partition("p"))?;
    assert_eq!(field(&head, "state"), text("queued"));
    let follower = get(
        &node,
        "follower",
        &[("partition_key", text("p")), ("full", Term::Bool(true))],
    )?;
    assert_eq!(field(&follower, "state"), text("failed"));
    assert_eq!(
        field(&field(&follower, "error"), "reason"),
        text("max_active_ms")
    );
    Ok(())
}

#[test]
fn a_cancelled_middle_entry_leaves_the_lane_order() -> TestResult {
    // Elixir: "fifo unlinks a cancelled middle entry without disturbing
    // lane order".
    let node = node("audit-rest-fifo-middle-cancel")?;
    fifo(&node, &["queued"], &[])?;
    for (id, now) in [("first", 1_000), ("second", 1_001), ("third", 1_002)] {
        create(&node, id, "p", now, 2_000)?;
    }
    ok(
        &node,
        "FLOW.CANCEL",
        &[
            ("id", text("second")),
            ("partition_key", text("p")),
            ("fencing_token", int(0)),
            ("now_ms", int(1_500)),
        ],
    )?;
    let first = claim(
        &node,
        &[("partition_key", text("p")), ("limit", int(1))],
        2_000,
        "w",
    )?;
    assert_eq!(ids(&first), texts(&["first"]));
    complete(&node, first.first().ok_or("one claimed")?, 2_100)?;
    let third = claim(
        &node,
        &[("partition_key", text("p")), ("limit", int(1))],
        2_101,
        "w",
    )?;
    assert_eq!(ids(&third), texts(&["third"]));
    Ok(())
}

#[test]
fn fifo_entries_need_a_partition_and_no_priority_or_running_entry() -> TestResult {
    // Elixir: "fifo state rejects priority and implicit partition entry"
    // and "fifo state rejects direct running entry".
    let node = node("audit-rest-fifo-entry-rules")?;
    fifo(&node, &["queued", "ready", "charge"], &[])?;
    let create_in = |id: &str, extra: &[(&str, Term)]| {
        let mut entries = vec![
            ("id", text(id)),
            ("type", text(KIND)),
            ("state", text("queued")),
            ("now_ms", int(1_000)),
            ("run_at_ms", int(1_000)),
        ];
        entries.extend(extra.iter().cloned());
        run(&node, "FLOW.CREATE", &entries)
    };
    assert_eq!(
        refusal(create_in("no-partition", &[])?)?,
        "ERR flow partition_key is required for fifo state"
    );
    assert_eq!(
        refusal(create_in(
            "priority",
            &[("partition_key", text("p")), ("priority", int(1))]
        )?)?,
        "ERR flow priority is not supported for fifo state"
    );
    create(&node, "moving", "p", 1_000, 1_000)?;
    let claimed = claim(
        &node,
        &[("partition_key", text("p")), ("limit", int(1))],
        1_000,
        "w",
    )?;
    let head = claimed.first().ok_or("one claimed")?;
    let lease = |extra: &[(&'static str, Term)]| -> Vec<(&'static str, Term)> {
        let mut entries = vec![
            ("id", text("moving")),
            ("partition_key", text("p")),
            ("lease_token", field(head, "lease_token")),
            ("fencing_token", field(head, "fencing_token")),
            ("now_ms", int(1_100)),
        ];
        entries.extend(extra.iter().cloned());
        entries
    };
    let moved = run(
        &node,
        "FLOW.TRANSITION",
        &lease(&[
            ("from_state", text("running")),
            ("to_state", text("ready")),
            ("priority", int(1)),
            ("run_at_ms", int(1_100)),
        ]),
    )?;
    assert_eq!(
        refusal(moved)?,
        "ERR flow priority is not supported for fifo state"
    );
    let direct = "ERR flow direct running entry is not supported for fifo state";
    let stepped = run(
        &node,
        "FLOW.STEP_CONTINUE",
        &lease(&[
            ("from_state", text("queued")),
            ("to_state", text("charge")),
            ("worker", text("w")),
        ]),
    )?;
    assert_eq!(refusal(stepped)?, direct);
    let started = run(
        &node,
        "FLOW.START_AND_CLAIM",
        &[
            ("id", text("started")),
            ("type", text(KIND)),
            ("initial_state", text("charge")),
            ("partition_key", text("p")),
            ("worker", text("w")),
            ("now_ms", int(1_000)),
        ],
    )?;
    assert_eq!(refusal(started)?, direct);
    Ok(())
}
