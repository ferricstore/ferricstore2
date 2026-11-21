//! `flow_test/sections/flow_policy_exposes_type_state_retry_retention_
//! defaults.exs`, its lease tests: an expired lease reclaimed, a lease
//! extended under its fence, and the takeover history.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::policy_lifecycle::{claim_one, create, int_of, lease, plus};
use super::{claim, claimed, events, field, get, int, ok, refusal, run, text};
use crate::{TestResult, directory, open};

#[test]
fn an_expired_lease_is_reclaimed_by_a_running_claim_or_reclaim() -> TestResult {
    // Elixir: "expired running lease can be reclaimed" and "flow_reclaim
    // exposes expired running lease reclaim".
    let node = open(&directory("audit-policy-reclaim")?, 4)?;
    let short = [("lease_ms", int(50))];
    create(
        &node,
        "r",
        "lease",
        &[("state", text("queued")), ("run_at_ms", int(1_000))],
    )?;
    let first = claim_one(
        &node,
        "lease",
        1_000,
        &[("state", text("queued")), ("lease_ms", int(50))],
    )?;
    assert_eq!(field(&first, "lease_deadline_ms"), int(1_050));
    let running = |now: i64| {
        let entries = [
            ("type", text("lease")),
            ("state", text("running")),
            ("worker", text("worker-b")),
            ("lease_ms", int(50)),
            ("limit", int(1)),
            ("now_ms", int(now)),
        ];
        claimed(&node, &entries)
    };
    assert_eq!(running(1_049)?, Vec::<Term>::new());
    let second = running(1_050)?.pop().ok_or("nothing reclaimed")?;
    assert_eq!(field(&second, "id"), text("r"));
    assert_eq!(field(&second, "lease_owner"), text("worker-b"));
    assert_eq!(field(&second, "version"), int(3));
    assert_ne!(field(&second, "lease_token"), field(&first, "lease_token"));

    create(
        &node,
        "api",
        "lease-api",
        &[("state", text("queued")), ("run_at_ms", int(1_000))],
    )?;
    let first = claim_one(&node, "lease-api", 1_000, &short)?;
    let reclaim = |now: i64| -> TestResult<Vec<Term>> {
        let entries = [
            ("type", text("lease-api")),
            ("worker", text("worker-b")),
            ("lease_ms", int(50)),
            ("limit", int(1)),
            ("now_ms", int(now)),
        ];
        match claim(&node, "FLOW.RECLAIM", &entries)?? {
            Term::List(records) => Ok(records),
            other => Err(format!("reclaim reply is not a list: {other:?}").into()),
        }
    };
    assert_eq!(reclaim(1_049)?, Vec::<Term>::new());
    let second = reclaim(1_050)?.pop().ok_or("nothing reclaimed")?;
    assert_eq!(field(&second, "id"), text("api"));
    assert_eq!(field(&second, "lease_owner"), text("worker-b"));
    assert_ne!(field(&second, "lease_token"), field(&first, "lease_token"));
    Ok(())
}

fn extend(
    node: &QueueNode,
    record: &Term,
    fence: i64,
    extra: &[(&str, Term)],
) -> TestResult<Result<Term, String>> {
    let mut entries = vec![
        ("id", field(record, "id")),
        ("lease_token", field(record, "lease_token")),
        ("fencing_token", int(fence)),
        ("lease_ms", int(500)),
    ];
    entries.extend(extra.iter().cloned());
    Ok(run(node, "FLOW.EXTEND_LEASE", &entries)?.map_err(|error| error.to_string()))
}

#[test]
fn a_lease_extends_under_its_fence_and_not_once_finished() -> TestResult {
    // Elixir: "flow_extend_lease extends running lease with fencing
    // guards", "flow_extend_lease can return OK without materializing the
    // updated record" and "flow_extend_lease rejects terminal flow".
    let node = open(&directory("audit-policy-extend")?, 4)?;
    let short = [("lease_ms", int(50))];
    create(&node, "x", "lease-extend", &[("run_at_ms", int(1_000))])?;
    let leased = claim_one(&node, "lease-extend", 1_000, &short)?;
    let fence = int_of(&field(&leased, "fencing_token"))?;
    let version = int_of(&field(&leased, "version"))?;
    let Ok(extended) = extend(&node, &leased, fence, &[("now_ms", int(1_020))])? else {
        return Err("the extension was refused".into());
    };
    assert_eq!(field(&extended, "state"), text("running"));
    assert_eq!(field(&extended, "version"), int(version + 1));
    assert_eq!(
        field(&extended, "lease_token"),
        field(&leased, "lease_token")
    );
    assert_eq!(field(&extended, "fencing_token"), int(fence));
    assert_eq!(field(&extended, "lease_deadline_ms"), int(1_520));
    let reclaimed = claim(
        &node,
        "FLOW.RECLAIM",
        &[
            ("type", text("lease-extend")),
            ("worker", text("worker-b")),
            ("lease_ms", int(50)),
            ("limit", int(1)),
            ("now_ms", int(1_100)),
        ],
    )??;
    assert_eq!(reclaimed, Term::List(Vec::new()));
    let stale = extend(&node, &leased, fence + 1, &[("now_ms", int(1_030))])?;
    assert_eq!(stale, Err("ERR stale flow lease".to_owned()));

    create(&node, "ok", "lease-extend-ok", &[("run_at_ms", int(1_000))])?;
    let leased = claim_one(&node, "lease-extend-ok", 1_000, &short)?;
    let fence = int_of(&field(&leased, "fencing_token"))?;
    let quiet = [("now_ms", int(1_020)), ("return", text("ok_on_success"))];
    assert_eq!(extend(&node, &leased, fence, &quiet)?, Ok(text("OK")));
    let extended = get(&node, "ok", &[])?;
    assert_eq!(
        field(&extended, "version"),
        int(int_of(&field(&leased, "version"))? + 1)
    );
    assert_eq!(field(&extended, "lease_deadline_ms"), int(1_520));

    create(
        &node,
        "done",
        "lease-extend-terminal",
        &[("run_at_ms", int(1_000))],
    )?;
    let leased = claim_one(&node, "lease-extend-terminal", 1_000, &short)?;
    let fence = int_of(&field(&leased, "fencing_token"))?;
    ok(
        &node,
        "FLOW.COMPLETE",
        &plus(lease(&leased), &[("now_ms", int(1_010))]),
    )?;
    let finished = extend(&node, &leased, fence, &[("now_ms", int(1_020))])?;
    assert_eq!(finished, Err("ERR stale flow lease".to_owned()));
    Ok(())
}

#[test]
fn a_reclaim_fences_the_old_worker_and_records_the_takeover() -> TestResult {
    // Elixir: "expired lease reclaim fences old worker and records takeover
    // history".
    let node = open(&directory("audit-policy-takeover")?, 4)?;
    create(&node, "t", "lease-fence", &[("run_at_ms", int(1_000))])?;
    let first = claim_one(&node, "lease-fence", 1_000, &[("lease_ms", int(50))])?;
    let reclaimed = claim(
        &node,
        "FLOW.RECLAIM",
        &[
            ("type", text("lease-fence")),
            ("worker", text("worker-b")),
            ("lease_ms", int(500)),
            ("limit", int(1)),
            ("now_ms", int(1_050)),
        ],
    )??;
    let Term::List(mut records) = reclaimed else {
        return Err("reclaim reply is not a list".into());
    };
    let second = records.pop().ok_or("nothing reclaimed")?;
    let fence = int_of(&field(&first, "fencing_token"))?;
    assert_eq!(field(&second, "id"), text("t"));
    assert_eq!(field(&second, "lease_owner"), text("worker-b"));
    assert_eq!(field(&second, "fencing_token"), int(fence + 1));
    assert_ne!(field(&second, "lease_token"), field(&first, "lease_token"));
    let late = run(
        &node,
        "FLOW.COMPLETE",
        &plus(lease(&first), &[("now_ms", int(1_060))]),
    )?;
    assert_eq!(refusal(late)?, "ERR stale flow lease");
    let extended = extend(&node, &first, fence, &[("now_ms", int(1_060))])?;
    assert_eq!(extended, Err("ERR stale flow lease".to_owned()));
    let names: Vec<Term> = events(&node, "t", &[("count", int(10))])?
        .iter()
        .map(|fields| field(fields, "event"))
        .collect();
    let claims = names
        .iter()
        .filter(|name| **name == text("claimed"))
        .count();
    let creates = names
        .iter()
        .filter(|name| **name == text("created"))
        .count();
    assert_eq!((names.len(), creates, claims), (3, 1, 2));
    let takeover = events(&node, "t", &[("count", int(10))])?
        .into_iter()
        .any(|fields| {
            field(&fields, "event") == text("claimed")
                && field(&fields, "lease_owner") == text("worker-b")
        });
    assert!(takeover);
    Ok(())
}
