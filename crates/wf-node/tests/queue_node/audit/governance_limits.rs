//! `flow_governance_limit_correctness_test.exs`, through the wire: the
//! credit a governed claim takes is released once by whatever ends its run
//! (complete, retry, fail, cancel, a batch or an independent batch), never
//! to a scope the caller names; extending the Flow's lease renews the
//! credit lease; running policies set per state govern claims, the
//! strictest one when a claim spans states; and a public release names
//! its exact reservation ids. (Elixir's limit cache, reconciler, record
//! codec and type-level policies are its own.)

use std::time::Duration;

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claim, claimed, field, int, map, ok, run, text, texts};
use crate::{TestResult, directory, open};

const P: &str = "limited";

fn create_due(node: &QueueNode, flow_type: &str, ids: &[&str], state: &str) -> TestResult {
    for id in ids {
        ok(
            node,
            "FLOW.CREATE",
            &[
                ("id", text(id)),
                ("type", text(flow_type)),
                ("state", text(state)),
                ("partition_key", text(P)),
                ("run_at_ms", int(1_000)),
                ("now_ms", int(1_000)),
            ],
        )?;
    }
    Ok(())
}

fn lease(node: &QueueNode, scope: &str, credits: i64, ttl: i64) -> TestResult {
    ok(
        node,
        "FLOW.LIMIT.LEASE",
        &[
            ("scope", text(scope)),
            ("shard_id", int(0)),
            ("amount", int(credits)),
            ("limit", int(credits)),
            ("ttl_ms", int(ttl)),
            ("now_ms", int(1_000)),
        ],
    )?;
    Ok(())
}

fn claim_entries(flow_type: &str, scope: Option<&str>, now: i64) -> Vec<(&'static str, Term)> {
    let mut entries = vec![
        ("type", text(flow_type)),
        ("state", text("queued")),
        ("partition_key", text(P)),
        ("worker", text("worker-0")),
        ("limit", int(1)),
        ("lease_ms", int(1_000)),
        ("now_ms", int(now)),
    ];
    if let Some(scope) = scope {
        entries.extend([
            ("governance_limit_scope", text(scope)),
            ("governance_shard_id", int(0)),
        ]);
    }
    entries
}

fn claim_one(node: &QueueNode, flow_type: &str, scope: Option<&str>, now: i64) -> TestResult<Term> {
    let records = claimed(node, &claim_entries(flow_type, scope, now))?;
    records
        .into_iter()
        .next()
        .ok_or_else(|| "nothing claimed".into())
}

fn refused_code(node: &QueueNode, entries: &[(&str, Term)]) -> TestResult<String> {
    match claim(node, "FLOW.CLAIM_DUE", entries)? {
        Ok(term) => Err(format!("a refused claim expected, got {term:?}").into()),
        Err(wf_application::commands::queue::ServiceError::Governance(denial)) => {
            Ok(String::from_utf8_lossy(denial.code()).into_owned())
        }
        Err(error) => Ok(error.to_string()),
    }
}

fn lease_of(node: &QueueNode, scope: &str) -> TestResult<Term> {
    let owner = ok(
        node,
        "FLOW.LIMIT.GET",
        &[("scope", text(scope)), ("now_ms", int(1_100))],
    )?;
    Ok(field(&field(&owner, "leases"), "0"))
}

/// Waits for `scope`'s credits in use to reach `expected`, as releases land
/// once their round is durable.
fn in_use(node: &QueueNode, scope: &str, expected: i64) -> TestResult {
    for _ in 0..100 {
        if field(&lease_of(node, scope)?, "in_use") == int(expected) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        field(&lease_of(node, scope)?, "in_use"),
        int(expected),
        "{scope}"
    );
    Ok(())
}

fn lease_fields(record: &Term) -> Vec<(&'static str, Term)> {
    vec![
        ("id", field(record, "id")),
        ("partition_key", text(P)),
        ("lease_token", field(record, "lease_token")),
        ("fencing_token", field(record, "fencing_token")),
    ]
}

fn with(
    base: Vec<(&'static str, Term)>,
    extra: &[(&'static str, Term)],
) -> Vec<(&'static str, Term)> {
    let mut entries = base;
    entries.extend(extra.iter().cloned());
    entries
}

#[test]
fn a_terminal_caller_cannot_redirect_its_release() -> TestResult {
    // Elixir: "terminal caller cannot redirect release to another governance
    // scope" and "bound governance reservation is not exposed in public Flow
    // records".
    let node = open(&directory("audit-limits-redirect")?, 4)?;
    create_due(&node, "redirect", &["redirected"], "queued")?;
    lease(&node, "claimed-scope", 1, 30_000)?;
    lease(&node, "unrelated-scope", 1, 30_000)?;
    let job = claim_one(&node, "redirect", Some("claimed-scope"), 1_001)?;
    let record = super::get(&node, "redirected", &[("partition_key", text(P))])?;
    assert_eq!(field(&record, "governance_limit"), Term::Nil);
    ok(
        &node,
        "FLOW.LIMIT.SPEND",
        &[
            ("scope", text("unrelated-scope")),
            ("shard_id", int(0)),
            ("amount", int(1)),
            ("now_ms", int(1_001)),
        ],
    )?;
    ok(
        &node,
        "FLOW.COMPLETE",
        &with(
            lease_fields(&job),
            &[
                ("now_ms", int(1_002)),
                ("governance_limit_scope", text("unrelated-scope")),
                ("governance_shard_id", int(0)),
            ],
        ),
    )?;
    in_use(&node, "claimed-scope", 0)?;
    in_use(&node, "unrelated-scope", 1)?;
    Ok(())
}

#[test]
fn every_terminal_move_releases_its_credit_once() -> TestResult {
    // Elixir: "terminal completion releases the reservation captured by
    // claim", "duplicate terminal completion releases a reservation exactly
    // once", "retry, fail, and cancel release their captured reservation"
    // and "batched and independent completion release only committed
    // reservations".
    let node = open(&directory("audit-limits-once")?, 4)?;
    create_due(
        &node,
        "once",
        &["once-1", "once-2", "once-3", "once-4"],
        "queued",
    )?;
    lease(&node, "once", 2, 30_000)?;
    let first = claim_one(&node, "once", Some("once"), 1_001)?;
    claim_one(&node, "once", Some("once"), 1_002)?;
    let done = with(lease_fields(&first), &[("now_ms", int(1_003))]);
    ok(&node, "FLOW.COMPLETE", &done)?;
    let _replayed = run(&node, "FLOW.COMPLETE", &done)?;
    in_use(&node, "once", 1)?;
    claim_one(&node, "once", Some("once"), 1_004)?;
    let over = claim_entries("once", Some("once"), 1_005);
    assert_eq!(refused_code(&node, &over)?, "GOVERNANCE_LIMIT_EXCEEDED");
    for name in ["FLOW.RETRY", "FLOW.FAIL", "FLOW.CANCEL"] {
        let flow_type = format!("ended-{}", name.trim_start_matches("FLOW."));
        let scope = format!("{flow_type}-scope");
        create_due(&node, &flow_type, &[&format!("{flow_type}-1")], "queued")?;
        lease(&node, &scope, 1, 30_000)?;
        let job = claim_one(&node, &flow_type, Some(&scope), 1_001)?;
        ok(
            &node,
            name,
            &with(lease_fields(&job), &[("now_ms", int(1_002))]),
        )?;
        in_use(&node, &scope, 0)?;
    }
    for independent in [false, true] {
        let flow_type = format!("many-{independent}");
        let ids: Vec<String> = (1..=3)
            .map(|index| format!("{flow_type}-{index}"))
            .collect();
        let names: Vec<&str> = ids.iter().map(String::as_str).collect();
        create_due(&node, &flow_type, &names, "queued")?;
        lease(&node, &flow_type, 3, 30_000)?;
        let mut jobs = Vec::new();
        for now in 1_001..=1_003 {
            jobs.push(claim_one(&node, &flow_type, Some(&flow_type), now)?);
        }
        let items: Vec<Term> = jobs
            .iter()
            .take(2)
            .map(|job| {
                map(&[
                    ("id", field(job, "id")),
                    ("lease_token", field(job, "lease_token")),
                    ("fencing_token", field(job, "fencing_token")),
                ])
            })
            .collect();
        let batch = [
            ("partition_key", text(P)),
            ("items", Term::List(items)),
            ("independent", Term::Bool(independent)),
            ("now_ms", int(1_004)),
        ];
        ok(&node, "FLOW.COMPLETE_MANY", &batch)?;
        in_use(&node, &flow_type, 1)?;
        let _replayed = run(&node, "FLOW.COMPLETE_MANY", &batch)?;
        std::thread::sleep(Duration::from_millis(50));
        in_use(&node, &flow_type, 1)?;
    }
    Ok(())
}

#[test]
fn extending_a_flow_s_lease_renews_its_credit_lease() -> TestResult {
    // Elixir: "Flow lease extension renews the bound governance credit
    // lease".
    let node = open(&directory("audit-limits-renew")?, 4)?;
    create_due(&node, "renew", &["renewed"], "queued")?;
    lease(&node, "renew", 1, 100)?;
    let mut entries = claim_entries("renew", Some("renew"), 1_001);
    entries.retain(|(key, _)| *key != "lease_ms");
    entries.push(("lease_ms", int(100)));
    let job = claimed(&node, &entries)?
        .into_iter()
        .next()
        .ok_or("nothing claimed")?;
    let extended = ok(
        &node,
        "FLOW.EXTEND_LEASE",
        &with(
            lease_fields(&job),
            &[("lease_ms", int(1_000)), ("now_ms", int(1_050))],
        ),
    )?;
    let deadline = field(&extended, "lease_deadline_ms");
    assert_eq!(deadline, int(2_050));
    // The renewal lands once the extension is durable.
    let mut held = Term::Nil;
    for _ in 0..100 {
        let owner = ok(
            &node,
            "FLOW.LIMIT.GET",
            &[("scope", text("renew")), ("now_ms", int(1_101))],
        )?;
        held = field(&field(&owner, "leases"), "0");
        if held != Term::Nil {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(field(&held, "in_use"), int(1));
    let Term::Int(expires) = field(&held, "expires_at_ms") else {
        return Err(format!("a lease deadline expected in {held:?}").into());
    };
    assert!(expires >= 2_050, "the credit lease ends at {expires}");
    Ok(())
}

fn running_limit(limit: i64, enforcement: &str, lease_size: i64) -> Term {
    let rule = map(&[
        ("limit", int(limit)),
        ("enforcement", text(enforcement)),
        ("lease_size", int(lease_size)),
    ]);
    map(&[("limits", map(&[("running", rule)]))])
}

fn govern_states(node: &QueueNode, flow_type: &str, states: &[(&str, Term)]) -> TestResult {
    let states: Vec<(&str, Term)> = states
        .iter()
        .map(|(state, governance)| (*state, map(&[("governance", governance.clone())])))
        .collect();
    ok(
        node,
        "FLOW.POLICY.SET",
        &[("type", text(flow_type)), ("states", map(&states))],
    )?;
    Ok(())
}

#[test]
fn running_policies_govern_claims_and_the_strictest_one_wins() -> TestResult {
    // Elixir: "strict global running policy enforces claims without caller
    // limit options", "approximate global running policy bounds prefetched
    // claims and recycles completion" and "multi-state claims enforce the
    // strictest resolved running policy", with the rules on the claimed
    // states, as the wire sets them.
    let node = open(&directory("audit-limits-policies")?, 4)?;
    create_due(&node, "strict", &["strict-1", "strict-2"], "queued")?;
    govern_states(
        &node,
        "strict",
        &[("queued", running_limit(1, "strict_global", 1))],
    )?;
    let first = claim_one(&node, "strict", None, 1_001)?;
    let next = claim_entries("strict", None, 1_002);
    assert_eq!(refused_code(&node, &next)?, "GOVERNANCE_LIMIT_EXCEEDED");
    ok(
        &node,
        "FLOW.COMPLETE",
        &with(lease_fields(&first), &[("now_ms", int(1_003))]),
    )?;
    std::thread::sleep(Duration::from_millis(50));
    claim_one(&node, "strict", None, 1_004)?;
    let ids: Vec<String> = (1..=5).map(|index| format!("approx-{index}")).collect();
    let names: Vec<&str> = ids.iter().map(String::as_str).collect();
    create_due(&node, "approx", &names, "queued")?;
    govern_states(
        &node,
        "approx",
        &[("queued", running_limit(4, "approximate_global", 4))],
    )?;
    let mut held = Vec::new();
    for offset in 0..4 {
        held.push(claim_one(&node, "approx", None, 1_001 + offset)?);
    }
    let over = claim_entries("approx", None, 1_005);
    assert_eq!(refused_code(&node, &over)?, "GOVERNANCE_LIMIT_EXCEEDED");
    let done = held.first().ok_or("nothing held")?;
    ok(
        &node,
        "FLOW.COMPLETE",
        &with(lease_fields(done), &[("now_ms", int(1_006))]),
    )?;
    std::thread::sleep(Duration::from_millis(50));
    claim_one(&node, "approx", None, 1_007)?;
    create_due(&node, "mixed", &["mixed-queued"], "queued")?;
    create_due(&node, "mixed", &["mixed-retry"], "retry")?;
    govern_states(
        &node,
        "mixed",
        &[
            ("queued", running_limit(1, "strict_global", 1)),
            ("retry", running_limit(2, "approximate_global", 2)),
        ],
    )?;
    let mut both = claim_entries("mixed", None, 1_001);
    both.retain(|(key, _)| !matches!(*key, "state" | "limit"));
    both.extend([("states", texts(&["queued", "retry"])), ("limit", int(2))]);
    assert_eq!(refused_code(&node, &both)?, "GOVERNANCE_LIMIT_EXCEEDED");
    Ok(())
}

#[test]
fn a_public_release_names_its_exact_reservations() -> TestResult {
    // Elixir: "public limit release requires exact reservation ids".
    let node = open(&directory("audit-limits-release")?, 4)?;
    lease(&node, "exact", 2, 30_000)?;
    let spent = ok(
        &node,
        "FLOW.LIMIT.SPEND",
        &[
            ("scope", text("exact")),
            ("shard_id", int(0)),
            ("amount", int(2)),
            ("now_ms", int(1_001)),
        ],
    )?;
    let Term::List(ids) = field(&spent, "reservation_ids") else {
        return Err(format!("reservation ids expected in {spent:?}").into());
    };
    let [first, second] = ids.as_slice() else {
        return Err("two reservation ids expected".into());
    };
    assert_ne!(first, second);
    let unnamed = run(
        &node,
        "FLOW.LIMIT.RELEASE",
        &[
            ("scope", text("exact")),
            ("shard_id", int(0)),
            ("amount", int(1)),
        ],
    )?;
    assert_eq!(
        unnamed.err().map(|error| error.to_string()),
        Some("ERR flow limit reservation_ids must contain one unique id per credit".to_owned())
    );
    assert_eq!(field(&lease_of(&node, "exact")?, "in_use"), int(2));
    let release = [
        ("scope", text("exact")),
        ("shard_id", int(0)),
        ("reservation_ids", Term::List(vec![first.clone()])),
    ];
    for _ in 0..2 {
        let released = ok(&node, "FLOW.LIMIT.RELEASE", &release)?;
        assert_eq!(
            field(&field(&field(&released, "leases"), "0"), "in_use"),
            int(1)
        );
    }
    Ok(())
}

#[test]
fn a_governed_retry_still_wakes_a_waiting_claim() -> TestResult {
    // Elixir: "governed retry still wakes claim waiters after releasing its
    // reservation".
    let node = open(&directory("audit-limits-retry-wake")?, 4)?;
    create_due(&node, "waker", &["waking"], "queued")?;
    lease(&node, "waker", 1, 30_000)?;
    // A lease the waiting claim, on the node's clock, cannot reclaim.
    let mut entries = claim_entries("waker", Some("waker"), 1_001);
    entries.retain(|(key, _)| *key != "lease_ms");
    entries.push(("lease_ms", int(10_000_000_000_000)));
    let job = claimed(&node, &entries)?
        .into_iter()
        .next()
        .ok_or("nothing claimed")?;
    std::thread::scope(|scope| -> TestResult {
        let waiting = scope.spawn(|| {
            claimed(
                &node,
                &[
                    ("type", text("waker")),
                    ("state", text("queued")),
                    ("partition_key", text(P)),
                    ("worker", text("waiter")),
                    ("limit", int(1)),
                    ("block_ms", int(2_000)),
                ],
            )
            .map_err(|error| error.to_string())
        });
        std::thread::sleep(Duration::from_millis(100));
        ok(
            &node,
            "FLOW.RETRY",
            &with(
                lease_fields(&job),
                &[
                    ("now_ms", int(1_002)),
                    (
                        "retry",
                        map(&[
                            ("max_retries", int(3)),
                            (
                                "backoff",
                                map(&[
                                    ("kind", text("fixed")),
                                    ("base_ms", int(1)),
                                    ("max_ms", int(1)),
                                    ("jitter_pct", int(0)),
                                ]),
                            ),
                        ]),
                    ),
                ],
            ),
        )?;
        let woken = waiting.join().map_err(|_| "the waiting claim panicked")??;
        assert_eq!(woken.len(), 1);
        assert_eq!(field(&woken[0], "id"), text("waking"));
        Ok(())
    })?;
    in_use(&node, "waker", 0)?;
    Ok(())
}
