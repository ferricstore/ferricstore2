//! `flow_governance_exhaustive_review_test.exs` and
//! `flow_governance_exhaustive_store_review_test.exs`, their rules a client
//! meets: budget reservation ids, budget,
//! circuit, approval and limit times that may not run backward, approval
//! deadlines and expiry, lease deadlines, exact-integer and size bounds on
//! what is stored, and refusals that leave the record as it was. (Their
//! record codecs, outboxes, catalogs and cache sessions are Elixir's; the
//! active-reservation bound is `governance::tests`.)

use wf_application::commands::flow::Term;
use wf_application::commands::queue::ServiceError;
use wf_node::composition::QueueNode;

use super::{field, int, ok, run, text};
use crate::{TestResult, directory, open};

const MAX_EXACT: i64 = 9_007_199_254_740_991;

fn refused(node: &QueueNode, name: &str, entries: &[(&str, Term)]) -> TestResult<String> {
    match run(node, name, entries)? {
        Ok(term) => Err(format!("{name}: a refusal expected, got {term:?}").into()),
        Err(ServiceError::Governance(denial)) => {
            Ok(String::from_utf8_lossy(denial.code()).into_owned())
        }
        Err(error) => Ok(error.to_string()),
    }
}

fn reserve(scope: &str, amount: i64, id: &str, now: i64) -> Vec<(&'static str, Term)> {
    vec![
        ("scope", text(scope)),
        ("amount", int(amount)),
        ("limit", int(10)),
        ("window_ms", int(1_000)),
        ("reservation_id", text(id)),
        ("now_ms", int(now)),
    ]
}

#[test]
fn a_reservation_id_retries_but_is_never_reused() -> TestResult {
    // Elixir: "budget reservation retries are idempotent and cannot reuse a
    // reservation id".
    let node = open(&directory("audit-budget-ids")?, 4)?;
    let first = ok(&node, "FLOW.BUDGET.RESERVE", &reserve("ids", 3, "r1", 100))?;
    let retried = ok(&node, "FLOW.BUDGET.RESERVE", &reserve("ids", 3, "r1", 101))?;
    assert_eq!(field(&retried, "used"), int(3));
    assert_eq!(field(&retried, "used"), field(&first, "used"));
    assert_eq!(
        refused(&node, "FLOW.BUDGET.RESERVE", &reserve("ids", 4, "r1", 102))?,
        "ERR flow budget reservation id already exists"
    );
    ok(
        &node,
        "FLOW.BUDGET.RELEASE",
        &[
            ("scope", text("ids")),
            ("reservation_id", text("r1")),
            ("now_ms", int(103)),
        ],
    )?;
    assert_eq!(
        refused(&node, "FLOW.BUDGET.RESERVE", &reserve("ids", 3, "r1", 104))?,
        "ERR flow budget reservation id already exists"
    );
    assert_eq!(
        refused(
            &node,
            "FLOW.BUDGET.RESERVE",
            &reserve("ids", 1, &"r".repeat(257), 105)
        )?,
        "ERR flow budget reservation_id must be at most 256 bytes"
    );
    Ok(())
}

#[test]
fn budget_times_and_totals_never_run_backward_or_past_2_pow_53() -> TestResult {
    // Elixir: "budget transitions reject timestamps that move durable state
    // backward" and "budget commits reject aggregate usage outside the exact
    // durable range".
    let node = open(&directory("audit-budget-times")?, 4)?;
    ok(
        &node,
        "FLOW.BUDGET.RESERVE",
        &reserve("times", 1, "seed", 100),
    )?;
    assert_eq!(
        refused(
            &node,
            "FLOW.BUDGET.RESERVE",
            &reserve("times", 1, "stale", 99)
        )?,
        "ERR flow budget now_ms cannot precede window_start_ms"
    );
    ok(
        &node,
        "FLOW.BUDGET.RESERVE",
        &reserve("times", 1, "held", 200),
    )?;
    for name in ["FLOW.BUDGET.COMMIT", "FLOW.BUDGET.RELEASE"] {
        let mut entries = vec![
            ("scope", text("times")),
            ("reservation_id", text("held")),
            ("now_ms", int(199)),
        ];
        if name == "FLOW.BUDGET.COMMIT" {
            entries.push(("actual_amount", int(1)));
        }
        assert_eq!(
            refused(&node, name, &entries)?,
            "ERR flow budget now_ms cannot precede reservation",
            "{name}"
        );
    }
    let huge = |amount: i64, id: &str, now: i64| {
        vec![
            ("scope", text("huge")),
            ("amount", int(amount)),
            ("limit", int(MAX_EXACT)),
            ("window_ms", int(1_000)),
            ("reservation_id", text(id)),
            ("now_ms", int(now)),
        ]
    };
    ok(
        &node,
        "FLOW.BUDGET.RESERVE",
        &huge(MAX_EXACT - 1, "first", 1_000),
    )?;
    ok(
        &node,
        "FLOW.BUDGET.COMMIT",
        &[
            ("scope", text("huge")),
            ("reservation_id", text("first")),
            ("actual_amount", int(MAX_EXACT - 1)),
            ("now_ms", int(1_001)),
        ],
    )?;
    ok(&node, "FLOW.BUDGET.RESERVE", &huge(1, "second", 1_002))?;
    assert_eq!(
        refused(
            &node,
            "FLOW.BUDGET.COMMIT",
            &[
                ("scope", text("huge")),
                ("reservation_id", text("second")),
                ("actual_amount", int(MAX_EXACT)),
                ("now_ms", int(1_003)),
            ]
        )?,
        "ERR flow budget aggregate usage exceeds durable integer range"
    );
    let kept = ok(&node, "FLOW.BUDGET.GET", &[("scope", text("huge"))])?;
    assert_eq!(field(&kept, "used"), int(MAX_EXACT));
    Ok(())
}

#[test]
fn approval_deadlines_expire_once_and_times_never_run_backward() -> TestResult {
    // Elixir: "approval decisions atomically expire requests at their
    // deadline", "approval requests reject an absolute deadline that is not
    // in the future", "approval decisions reject time regression without
    // corrupting the record" and "approval records reject terminal
    // decisions at or after their expiry".
    let node = open(&directory("audit-approval-times")?, 4)?;
    let request = |id: &str, extra: &[(&'static str, Term)]| {
        let mut entries = vec![
            ("id", text(id)),
            ("flow_id", text("flow")),
            ("scope", text("deadline-scope")),
        ];
        entries.extend(extra.iter().cloned());
        entries
    };
    ok(
        &node,
        "FLOW.APPROVAL.REQUEST",
        &request(
            "deadline",
            &[("expires_at_ms", int(2_000)), ("now_ms", int(1_000))],
        ),
    )?;
    assert_eq!(
        refused(
            &node,
            "FLOW.APPROVAL.APPROVE",
            &[
                ("id", text("deadline")),
                ("approver", text("operator")),
                ("now_ms", int(2_000)),
            ]
        )?,
        "GOVERNANCE_CONFLICT"
    );
    let expired = ok(&node, "FLOW.APPROVAL.GET", &[("id", text("deadline"))])?;
    assert_eq!(
        [
            field(&expired, "status"),
            field(&expired, "decided_at_ms"),
            field(&expired, "decided_by"),
        ],
        [text("expired"), int(2_000), Term::Nil]
    );
    let Term::List(listed) = ok(
        &node,
        "FLOW.APPROVAL.LIST",
        &[
            ("status", text("expired")),
            ("scope", text("deadline-scope")),
        ],
    )?
    else {
        return Err("the listing is not a list".into());
    };
    assert_eq!(listed.len(), 1);
    assert_eq!(
        refused(
            &node,
            "FLOW.APPROVAL.REQUEST",
            &request(
                "stale",
                &[("expires_at_ms", int(1_000)), ("now_ms", int(1_000))]
            )
        )?,
        "ERR flow approval expires_at_ms must be greater than now_ms"
    );
    assert_eq!(
        ok(&node, "FLOW.APPROVAL.GET", &[("id", text("stale"))])?,
        Term::Nil
    );
    let requested = ok(
        &node,
        "FLOW.APPROVAL.REQUEST",
        &request("regress", &[("now_ms", int(2_000))]),
    )?;
    assert_eq!(
        refused(
            &node,
            "FLOW.APPROVAL.APPROVE",
            &[
                ("id", text("regress")),
                ("approver", text("operator")),
                ("now_ms", int(1_999)),
            ]
        )?,
        "ERR flow approval now_ms cannot precede requested_at_ms"
    );
    assert_eq!(
        ok(&node, "FLOW.APPROVAL.GET", &[("id", text("regress"))])?,
        requested
    );
    Ok(())
}

#[test]
fn approval_text_and_stored_integers_stay_bounded() -> TestResult {
    // Elixir: "approval metadata is bounded before record publication" and
    // "governance stores reject integers outside the exact durable range".
    let node = open(&directory("audit-governance-bounds")?, 4)?;
    let request = |id: &str, extra: &[(&'static str, Term)]| {
        let mut entries = vec![
            ("id", text(id)),
            ("flow_id", text("flow")),
            ("scope", text("scope")),
            ("now_ms", int(1_000)),
        ];
        entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
        entries.extend(extra.iter().cloned());
        entries
    };
    assert_eq!(
        refused(
            &node,
            "FLOW.APPROVAL.REQUEST",
            &request("long-reason", &[("reason", text(&"r".repeat(262_145)))])
        )?,
        "ERR flow approval reason must be at most 262144 bytes"
    );
    assert_eq!(
        ok(&node, "FLOW.APPROVAL.GET", &[("id", text("long-reason"))])?,
        Term::Nil
    );
    let pending = ok(&node, "FLOW.APPROVAL.REQUEST", &request("pending", &[]))?;
    assert_eq!(
        refused(
            &node,
            "FLOW.APPROVAL.APPROVE",
            &[
                ("id", text("pending")),
                ("approver", text(&"a".repeat(262_145))),
                ("now_ms", int(1_001)),
            ]
        )?,
        "ERR flow approval approver must be at most 262144 bytes"
    );
    assert_eq!(
        ok(&node, "FLOW.APPROVAL.GET", &[("id", text("pending"))])?,
        pending
    );
    refused(
        &node,
        "FLOW.APPROVAL.REQUEST",
        &request("exact", &[("now_ms", int(MAX_EXACT + 1))]),
    )?;
    refused(
        &node,
        "FLOW.BUDGET.RESERVE",
        &[
            ("scope", text("exact")),
            ("amount", int(MAX_EXACT + 1)),
            ("limit", int(MAX_EXACT + 1)),
            ("window_ms", int(1_000)),
            ("now_ms", int(1_000)),
        ],
    )?;
    refused(
        &node,
        "FLOW.CIRCUIT.OPEN",
        &[
            ("scope", text("exact")),
            ("now_ms", int(MAX_EXACT + 1)),
            ("open_ms", int(MAX_EXACT + 1)),
        ],
    )?;
    for (name, key) in [
        ("FLOW.APPROVAL.GET", "id"),
        ("FLOW.BUDGET.GET", "scope"),
        ("FLOW.CIRCUIT.GET", "scope"),
    ] {
        assert_eq!(
            ok(&node, name, &[(key, text("exact"))])?,
            Term::Nil,
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn circuit_and_lease_times_keep_their_order() -> TestResult {
    // Elixir: "circuit mutations reject timestamps older than durable
    // state", "credit lease initial grants set an exact deadline", "credit
    // lease top-ups never shorten an existing deadline" and "credit lease
    // spends reject timestamps older than the previous spend".
    let node = open(&directory("audit-governance-order")?, 4)?;
    let opened = ok(
        &node,
        "FLOW.CIRCUIT.OPEN",
        &[
            ("scope", text("monotonic")),
            ("failure_threshold", int(1)),
            ("open_ms", int(1_000)),
            ("now_ms", int(1_000)),
        ],
    )?;
    assert_eq!(field(&opened, "status"), text("open"));
    assert_eq!(
        refused(
            &node,
            "FLOW.CIRCUIT.CLOSE",
            &[("scope", text("monotonic")), ("now_ms", int(999))]
        )?,
        "ERR flow circuit now_ms cannot precede updated_at_ms"
    );
    let kept = ok(&node, "FLOW.CIRCUIT.GET", &[("scope", text("monotonic"))])?;
    assert_eq!(
        (field(&kept, "status"), field(&kept, "updated_at_ms")),
        (text("open"), int(1_000))
    );
    let lease = |amount: i64, ttl: i64, now: i64| {
        vec![
            ("scope", text("leases")),
            ("shard_id", int(0)),
            ("amount", int(amount)),
            ("limit", int(2)),
            ("ttl_ms", int(ttl)),
            ("now_ms", int(now)),
        ]
    };
    let granted = ok(&node, "FLOW.LIMIT.LEASE", &lease(1, 500, 500))?;
    assert_eq!(
        field(&field(&granted, "lease"), "expires_at_ms"),
        int(1_000)
    );
    let topped = ok(&node, "FLOW.LIMIT.LEASE", &lease(1, 100, 600))?;
    assert_eq!(field(&field(&topped, "lease"), "expires_at_ms"), int(1_000));
    let spend = |now: i64| {
        vec![
            ("scope", text("leases")),
            ("shard_id", int(0)),
            ("amount", int(1)),
            ("now_ms", int(now)),
        ]
    };
    ok(&node, "FLOW.LIMIT.SPEND", &spend(700))?;
    assert_eq!(
        refused(&node, "FLOW.LIMIT.SPEND", &spend(699))?,
        "ERR flow limit now_ms cannot precede last_spend_at_ms"
    );
    Ok(())
}

#[test]
fn oversized_governance_keys_are_refused_before_anything_is_stored() -> TestResult {
    // Elixir: "oversized governance storage keys are rejected before
    // catalog publication", "approval rejects oversized secondary catalog
    // dimensions before storage" and "exact governance lists reject
    // oversized derived keys".
    let node = open(&directory("audit-governance-keys")?, 4)?;
    let too_large = "ERR key too large (max 65535 bytes)";
    let at_limit = "k".repeat(65_535);
    let over = "x".repeat(65_536);
    for (name, entries) in [
        (
            "FLOW.APPROVAL.REQUEST",
            vec![
                ("id", text(&at_limit)),
                ("flow_id", text("flow")),
                ("scope", text("scope")),
                ("now_ms", int(1_000)),
            ],
        ),
        (
            "FLOW.BUDGET.RESERVE",
            vec![
                ("scope", text(&at_limit)),
                ("amount", int(1)),
                ("limit", int(1)),
                ("window_ms", int(1_000)),
                ("now_ms", int(1_000)),
            ],
        ),
        (
            "FLOW.CIRCUIT.OPEN",
            vec![("scope", text(&at_limit)), ("now_ms", int(1_000))],
        ),
        (
            "FLOW.LIMIT.LEASE",
            vec![
                ("scope", text(&at_limit)),
                ("shard_id", int(0)),
                ("amount", int(1)),
                ("limit", int(1)),
                ("ttl_ms", int(1_000)),
                ("now_ms", int(1_000)),
            ],
        ),
        (
            "FLOW.APPROVAL.REQUEST",
            vec![
                ("id", text("dimension-flow")),
                ("flow_id", text(&over)),
                ("scope", text("scope")),
                ("now_ms", int(1_000)),
            ],
        ),
        (
            "FLOW.APPROVAL.REQUEST",
            vec![
                ("id", text("dimension-scope")),
                ("flow_id", text("flow")),
                ("scope", text(&over)),
                ("now_ms", int(1_000)),
            ],
        ),
    ] {
        assert_eq!(refused(&node, name, &entries)?, too_large, "{name}");
    }
    for (name, key) in [
        ("FLOW.APPROVAL.LIST", "flow_id"),
        ("FLOW.APPROVAL.LIST", "scope"),
        ("FLOW.BUDGET.LIST", "scope"),
    ] {
        assert_eq!(
            refused(&node, name, &[(key, text(&over))])?,
            too_large,
            "{name} {key}"
        );
    }
    for id in ["dimension-flow", "dimension-scope"] {
        assert_eq!(
            ok(&node, "FLOW.APPROVAL.GET", &[("id", text(id))])?,
            Term::Nil
        );
    }
    Ok(())
}
