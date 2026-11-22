//! `flow_test/sections/flow_policy_exposes_type_state_retry_retention_
//! defaults.exs`: policies by type and state, retries under them, what a
//! restart keeps, batch retries, and leases reclaimed or extended.

use wf_application::commands::flow::{self, FlowService, Term};
use wf_node::composition::QueueNode;

use super::{claimed, field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

fn path(term: &Term, keys: &[&str]) -> Term {
    keys.iter()
        .fold(term.clone(), |value, key| field(&value, key))
}

fn has_path(term: &Term, keys: &[&str], last: &str) -> bool {
    super::has(&path(term, keys), last)
}

pub(super) fn int_of(term: &Term) -> TestResult<i64> {
    match term {
        Term::Int(value) => Ok(*value),
        other => Err(format!("not an integer: {other:?}").into()),
    }
}

pub(super) fn create(
    node: &QueueNode,
    id: &str,
    flow_type: &str,
    extra: &[(&str, Term)],
) -> TestResult {
    let mut entries = vec![("id", text(id)), ("type", text(flow_type))];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

/// The one Flow a leased of `flow_type` takes at `now`.
pub(super) fn claim_one(
    node: &QueueNode,
    flow_type: &str,
    now: i64,
    extra: &[(&str, Term)],
) -> TestResult<Term> {
    let mut entries = vec![
        ("type", text(flow_type)),
        ("worker", text("worker-a")),
        ("limit", int(1)),
        ("now_ms", int(now)),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    let records = claimed(node, &entries)?;
    let [record] = records.as_slice() else {
        return Err(format!("one leased expected, got {records:?}").into());
    };
    Ok(record.clone())
}

pub(super) fn lease(record: &Term) -> Vec<(&'static str, Term)> {
    vec![
        ("id", field(record, "id")),
        ("lease_token", field(record, "lease_token")),
        ("fencing_token", field(record, "fencing_token")),
    ]
}

pub(super) fn plus<'a>(
    mut entries: Vec<(&'a str, Term)>,
    extra: &[(&'a str, Term)],
) -> Vec<(&'a str, Term)> {
    entries.extend(extra.iter().cloned());
    entries
}

/// `FLOW.POLICY.SET` in the text form the SDK sends through `COMMAND_EXEC`.
pub(super) fn text_set(node: &QueueNode, args: &[Term]) -> TestResult<Term> {
    let parsed = flow::text::parse(b"FLOW.POLICY.SET", args)?;
    let flow::text::Parsed::Command(command) = parsed else {
        return Err("not a command".into());
    };
    Ok(crate::call(|reply| node.execute(command, reply))??)
}

fn retry_policy(max_retries: i64, exhausted_to: &str) -> Term {
    map(&[
        ("max_retries", int(max_retries)),
        ("exhausted_to", text(exhausted_to)),
    ])
}

fn fixed(base_ms: i64, max_ms: i64) -> Term {
    map(&[
        ("kind", text("fixed")),
        ("base_ms", int(base_ms)),
        ("max_ms", int(max_ms)),
        ("jitter_pct", int(0)),
    ])
}

#[test]
fn a_policy_resolves_type_defaults_under_state_overrides() -> TestResult {
    // Elixir: "flow policy exposes type and state retry and retention
    // defaults".
    let node = open(&directory("audit-policy-defaults")?, 4)?;
    let retry = map(&[
        ("max_retries", int(5)),
        ("backoff", fixed(10_000, 30_000)),
        ("exhausted_to", text("failed")),
    ]);
    let charge_card = map(&[
        ("retry", retry_policy(2, "payment_failed")),
        (
            "retention",
            map(&[("ttl_ms", int(30_000)), ("history_max_events", int(256))]),
        ),
    ]);
    ok(
        &node,
        "FLOW.POLICY.SET",
        &[
            ("type", text("flow-policy")),
            ("max_active_ms", int(120_000)),
            ("retry", retry),
            ("states", map(&[("charge_card", charge_card)])),
        ],
    )?;
    // Elixir's typed protocol has no top-level retention (`ERR native
    // unknown flow option retention`); the text form sets its time.
    let type_ttl = [text("flow-policy"), text("retention_ttl"), int(60_000)];
    let policy = text_set(&node, &type_ttl)?;
    assert_eq!(path(&policy, &["retry", "max_retries"]), int(5));
    assert_eq!(field(&policy, "max_active_ms"), int(120_000));
    assert_eq!(path(&policy, &["retention", "ttl_ms"]), int(60_000));
    assert!(!has_path(&policy, &["retention"], "history_hot_max_events"));
    let state = ["states", "charge_card"];
    let at = |rest: &[&str]| path(&policy, &[&state[..], rest].concat());
    assert_eq!(at(&["retry", "max_retries"]), int(2));
    assert_eq!(at(&["retry", "backoff", "kind"]), text("fixed"));
    assert_eq!(at(&["retry", "exhausted_to"]), text("payment_failed"));
    assert_eq!(at(&["retention", "ttl_ms"]), int(30_000));
    assert!(!super::has(&at(&["retention"]), "history_hot_max_events"));
    assert_eq!(at(&["retention", "history_max_events"]), int(256));
    let resolved = ok(
        &node,
        "FLOW.POLICY.GET",
        &[
            ("type", text("flow-policy")),
            ("state", text("charge_card")),
        ],
    )?;
    assert_eq!(field(&resolved, "max_active_ms"), int(120_000));
    assert_eq!(path(&resolved, &["retry", "max_retries"]), int(2));
    assert_eq!(
        path(&resolved, &["retry", "backoff", "base_ms"]),
        int(10_000)
    );
    assert_eq!(
        path(&resolved, &["retry", "exhausted_to"]),
        text("payment_failed")
    );
    assert_eq!(path(&resolved, &["retention", "ttl_ms"]), int(30_000));
    assert!(!has_path(
        &resolved,
        &["retention"],
        "history_hot_max_events"
    ));
    assert_eq!(
        path(&resolved, &["retention", "history_max_events"]),
        int(256)
    );
    Ok(())
}

#[test]
fn a_policy_refuses_internal_type_only_and_renamed_fields() -> TestResult {
    // Elixir: "flow policy rejects hot history cap because it is internal",
    // "flow policy rejects state-level max_active_ms because it is
    // type-level only" and "flow retry policy rejects old max_attempts
    // name".
    let node = open(&directory("audit-policy-refusals")?, 4)?;
    let set = |entries: &[(&str, Term)]| {
        let mut all = vec![("type", text("refused"))];
        all.extend(entries.iter().cloned());
        run(&node, "FLOW.POLICY.SET", &all)
    };
    let hot = map(&[("ttl_ms", int(5_000)), ("history_hot_max_events", int(3))]);
    let hot = map(&[("queued", map(&[("retention", hot)]))]);
    assert_eq!(
        refusal(set(&[("states", hot)])?)?,
        "ERR flow retention history_hot_max_events is internal"
    );
    let queued = map(&[("queued", map(&[("max_active_ms", int(1_000))]))]);
    assert_eq!(
        refusal(set(&[("states", queued)])?)?,
        "ERR flow max_active_ms is type-level only"
    );
    let renamed = map(&[
        ("max_attempts", int(3)),
        ("backoff", fixed(100, 1_000)),
        ("exhausted_to", text("failed")),
    ]);
    assert_eq!(
        refusal(set(&[("retry", renamed)])?)?,
        "ERR flow retry max_attempts was renamed to max_retries"
    );
    Ok(())
}

#[test]
fn a_retry_follows_the_state_policy_unless_the_command_overrides_it() -> TestResult {
    // Elixir: "stored state retry policy drives retry exhaustion without
    // command override" and "command retry policy overrides stored state
    // policy".
    let node = open(&directory("audit-policy-retry")?, 4)?;
    let states = map(&[(
        "charge_card",
        map(&[("retry", retry_policy(0, "payment_failed"))]),
    )]);
    ok(
        &node,
        "FLOW.POLICY.SET",
        &[
            ("type", text("exhaust")),
            ("retry", retry_policy(10, "failed")),
            ("states", states),
        ],
    )?;
    let in_card = [("state", text("charge_card")), ("run_at_ms", int(1_000))];
    create(&node, "e", "exhaust", &in_card)?;
    let leased = claim_one(&node, "exhaust", 1_000, &[("state", text("charge_card"))])?;
    ok(
        &node,
        "FLOW.RETRY",
        &plus(lease(&leased), &[("now_ms", int(2_000))]),
    )?;
    let exhausted = get(&node, "e", &[])?;
    assert_eq!(field(&exhausted, "state"), text("payment_failed"));
    assert_eq!(field(&exhausted, "next_run_at_ms"), int(2_000));

    create(&node, "o", "exhaust", &in_card)?;
    let leased = claim_one(&node, "exhaust", 1_000, &[("state", text("charge_card"))])?;
    let overridden = map(&[
        ("max_retries", int(3)),
        ("backoff", fixed(5_000, 5_000)),
        ("exhausted_to", text("needs_review")),
    ]);
    let retry = [("now_ms", int(2_000)), ("retry", overridden)];
    ok(&node, "FLOW.RETRY", &plus(lease(&leased), &retry))?;
    let retried = get(&node, "o", &[])?;
    assert_eq!(field(&retried, "state"), text("charge_card"));
    assert_eq!(field(&retried, "next_run_at_ms"), int(7_000));
    Ok(())
}

/// Before the restart: a stored policy, a Flow it will exhaust, a claimed
/// Flow and a parent waiting on a child group.
fn before_restart(node: &QueueNode) -> TestResult<Term> {
    let states = map(&[(
        "charge_card",
        map(&[("retry", retry_policy(0, "payment_failed"))]),
    )]);
    // `version` is Elixir's in-process option; its wire refuses it.
    let set = [("type", text("restart")), ("states", states)];
    assert_eq!(
        field(&ok(node, "FLOW.POLICY.SET", &set)?, "generation"),
        int(1)
    );
    let in_card = [
        ("state", text("charge_card")),
        ("run_at_ms", int(1_000)),
        ("now_ms", int(1_000)),
    ];
    create(node, "card", "restart", &in_card)?;
    let queued = [
        ("state", text("queued")),
        ("run_at_ms", int(1_000)),
        ("now_ms", int(1_000)),
    ];
    create(node, "held", "native-leased", &queued)?;
    let held = claim_one(
        node,
        "native-leased",
        1_000,
        &[("worker", text("worker-native-restart"))],
    )?;
    assert_eq!(field(&held, "state"), text("running"));
    let family = [("partition_key", text("tenant-restart"))];
    let mut parent = family.to_vec();
    parent.extend([("state", text("dispatch")), ("now_ms", int(1_000))]);
    create(node, "parent", "parent", &parent)?;
    let fence = field(&get(node, "parent", &family)?, "fencing_token");
    let child = map(&[
        ("id", text("child")),
        ("type", text("child")),
        ("state", text("queued")),
    ]);
    ok(
        node,
        "FLOW.SPAWN_CHILDREN",
        &[
            ("id", text("parent")),
            ("partition_key", text("tenant-restart")),
            ("children", Term::List(vec![child])),
            ("group_id", text("fanout")),
            ("wait", text("all")),
            ("wait_state", text("waiting_children")),
            ("on_child_failed", text("fail_parent")),
            ("on_parent_closed", text("cancel_children")),
            (
                "exhaust_to",
                map(&[
                    ("success", text("children_done")),
                    ("failure", text("children_failed")),
                ]),
            ),
            ("from_state", text("dispatch")),
            ("fencing_token", fence),
            ("now_ms", int(1_010)),
        ],
    )?;
    Ok(held)
}

#[test]
fn a_restart_keeps_policies_leases_and_child_groups() -> TestResult {
    // Elixir: "standalone restart rebuilds stored policy and retry uses it",
    // "native leased batch state write survives standalone restart" and
    // "standalone restart preserves spawned child group state".
    let path = directory("audit-policy-restart")?;
    let held = before_restart(&open(&path, 4)?)?;
    let node = open(&path, 3)?;
    let state_policy = ok(
        &node,
        "FLOW.POLICY.GET",
        &[("type", text("restart")), ("state", text("charge_card"))],
    )?;
    assert_eq!(field(&state_policy, "generation"), int(1));
    assert_eq!(path_of(&state_policy, &["retry", "max_retries"]), int(0));
    let states = map(&[(
        "charge_card",
        map(&[("retry", retry_policy(0, "payment_failed"))]),
    )]);
    let set = [("type", text("restart")), ("states", states)];
    assert_eq!(
        field(&ok(&node, "FLOW.POLICY.SET", &set)?, "generation"),
        int(2)
    );
    let leased = claim_one(&node, "restart", 1_000, &[("state", text("charge_card"))])?;
    ok(
        &node,
        "FLOW.RETRY",
        &plus(lease(&leased), &[("now_ms", int(2_000))]),
    )?;
    let retried = get(&node, "card", &[])?;
    assert_eq!(field(&retried, "state"), text("payment_failed"));
    assert_eq!(field(&retried, "next_run_at_ms"), int(2_000));

    let restored = get(&node, "held", &[])?;
    assert_eq!(field(&restored, "state"), text("running"));
    assert_eq!(field(&restored, "lease_token"), field(&held, "lease_token"));
    assert_eq!(
        field(&restored, "fencing_token"),
        field(&held, "fencing_token")
    );
    assert_eq!(
        field(&restored, "lease_owner"),
        text("worker-native-restart")
    );

    let family = [("partition_key", text("tenant-restart"))];
    let parent = get(&node, "parent", &family)?;
    assert_eq!(field(&parent, "state"), text("waiting_children"));
    let group = path_of(&parent, &["child_groups", "fanout"]);
    assert_eq!(path_of(&group, &["children", "child"]), text("running"));
    assert_eq!(field(&group, "resolved"), Term::Nil);
    assert_eq!(
        field(&get(&node, "child", &family)?, "parent_flow_id"),
        text("parent")
    );
    Ok(())
}

fn path_of(term: &Term, keys: &[&str]) -> Term {
    path(term, keys)
}

const P: &str = "tenant-retry-many";

/// Creates `a` and `b` of `flow_type` in `P` and claims both, in id order.
/// `a` and `b` of the batch named `flow_type`.
fn pair(flow_type: &str) -> [String; 2] {
    [format!("{flow_type}-a"), format!("{flow_type}-b")]
}

fn two_claimed(node: &QueueNode, flow_type: &str) -> TestResult<Vec<Term>> {
    ok(
        node,
        "FLOW.CREATE_MANY",
        &[
            ("partition_key", text(P)),
            ("type", text(flow_type)),
            ("state", text("queued")),
            ("run_at_ms", int(1_000)),
            (
                "items",
                Term::List(pair(flow_type).iter().map(|id| text(id)).collect()),
            ),
        ],
    )?;
    let mut records = claimed(
        node,
        &[
            ("type", text(flow_type)),
            ("partition_key", text(P)),
            ("worker", text("worker-retry")),
            ("limit", int(2)),
            ("now_ms", int(1_000)),
        ],
    )?;
    records.sort_by_key(|record| format!("{:?}", field(record, "id")));
    Ok(records)
}

fn items(records: &[Term], bump: Option<&str>) -> TestResult<Term> {
    let mut items = Vec::new();
    for record in records {
        let mut fence = int_of(&field(record, "fencing_token"))?;
        if bump.is_some_and(|id| field(record, "id") == text(id)) {
            fence = fence.saturating_add(1);
        }
        items.push(map(&[
            ("id", field(record, "id")),
            ("lease_token", field(record, "lease_token")),
            ("fencing_token", int(fence)),
        ]));
    }
    Ok(Term::List(items))
}

#[test]
fn retry_many_reschedules_a_batch_or_none_of_it() -> TestResult {
    // Elixir: "flow_retry_many atomically reschedules one-partition batch",
    // "flow_retry_many rolls back when any item fails guard" and
    // "flow_retry_many rejects invalid retry policy before mutating
    // records".
    let node = open(&directory("audit-policy-retry-many")?, 4)?;
    let in_p = [("partition_key", text(P))];
    let records = two_claimed(&node, "bulk")?;
    let retry = [
        ("partition_key", text(P)),
        ("items", items(&records, None)?),
        ("error", text("retry-error")),
        ("run_at_ms", int(2_000)),
        ("now_ms", int(2_000)),
    ];
    assert_eq!(ok(&node, "FLOW.RETRY_MANY", &retry)?, text("OK"));
    for id in pair("bulk") {
        let record = get(&node, &id, &in_p)?;
        assert_eq!(field(&record, "state"), text("queued"));
        assert_eq!(field(&record, "attempts"), int(1));
        let error_ref = field(&record, "error_ref");
        assert!(matches!(&error_ref, Term::Bin(bytes) if bytes.as_slice() != b"retry-error"));
    }
    let again = claimed(
        &node,
        &[
            ("type", text("bulk")),
            ("partition_key", text(P)),
            ("worker", text("worker-retry-b")),
            ("limit", int(2)),
            ("now_ms", int(2_000)),
        ],
    )?;
    assert_eq!(again.len(), 2);

    let records = two_claimed(&node, "bulk-rollback")?;
    let stale = [
        ("partition_key", text(P)),
        ("items", items(&records, Some("bulk-rollback-b"))?),
        ("run_at_ms", int(2_000)),
        ("now_ms", int(2_000)),
    ];
    assert_eq!(
        refusal(run(&node, "FLOW.RETRY_MANY", &stale)?)?,
        "ERR stale flow lease"
    );
    let records2 = two_claimed_check(&node, &in_p, "bulk-rollback")?;
    assert_eq!(records2, 2);

    let records = two_claimed(&node, "bulk-policy")?;
    let invalid = [
        ("partition_key", text(P)),
        ("items", items(&records, None)?),
        ("retry", map(&[("max_retries", int(1_001))])),
        ("now_ms", int(2_000)),
    ];
    assert_eq!(
        refusal(run(&node, "FLOW.RETRY_MANY", &invalid)?)?,
        "ERR flow retry max_retries must be between 0 and 1000"
    );
    two_claimed_check(&node, &in_p, "bulk-policy")?;
    Ok(())
}

/// Both Flows of `batch` in `P` are still running as claimed.
fn two_claimed_check(node: &QueueNode, in_p: &[(&str, Term)], batch: &str) -> TestResult<usize> {
    for id in pair(batch) {
        let record = get(node, &id, in_p)?;
        assert_eq!(field(&record, "state"), text("running"), "{id}");
        assert_eq!(field(&record, "version"), int(2), "{id}");
        assert_eq!(field(&record, "attempts"), int(0), "{id}");
    }
    Ok(2)
}
