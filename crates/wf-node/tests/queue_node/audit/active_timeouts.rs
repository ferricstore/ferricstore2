//! `flow_test/sections/flow_max_active_ms_times_out_flows.exs`: a Flow past
//! its active-time limit fails (in a cleanup pass, a claim or a completion),
//! starts its retention and fails its parent's group; cleanup shares one
//! limit between timeouts and removals. Times start a minute ahead of the
//! wall clock, as Elixir's do, so the shard's own sweeps leave them alone.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, events, field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

fn ahead() -> TestResult<i64> {
    let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
    Ok(i64::try_from(wall.as_millis())? + 60_000)
}

fn create(node: &QueueNode, id: &str, flow_type: &str, extra: &[(&str, Term)]) -> TestResult {
    let mut entries = vec![
        ("id", text(id)),
        ("type", text(flow_type)),
        ("state", text("queued")),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

fn cleanup(node: &QueueNode, limit: i64, now: i64) -> TestResult<Term> {
    ok(
        node,
        "FLOW.RETENTION_CLEANUP",
        &[("limit", int(limit)), ("now_ms", int(now))],
    )
}

fn claim_at(
    node: &QueueNode,
    flow_type: &str,
    now: i64,
    extra: &[(&str, Term)],
) -> TestResult<Vec<Term>> {
    let mut entries = vec![
        ("type", text(flow_type)),
        ("worker", text("worker-active-timeout")),
        ("limit", int(1)),
        ("now_ms", int(now)),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    claimed(node, &entries)
}

fn timed_out(limit: i64) -> Term {
    map(&[
        ("reason", text("max_active_ms")),
        ("max_active_ms", int(limit)),
    ])
}

#[test]
fn a_flow_without_a_limit_never_times_out() -> TestResult {
    // Elixir: "max_active_ms defaults to infinite and does not affect active
    // flows when unset".
    let node = open(&directory("audit-timeout-unset")?, 4)?;
    let now = ahead()?;
    create(
        &node,
        "u",
        "unset",
        &[("run_at_ms", int(now)), ("now_ms", int(now))],
    )?;
    assert_eq!(field(&get(&node, "u", &[])?, "max_active_ms"), Term::Nil);
    let pass = cleanup(&node, 10, now + 86_400_000)?;
    assert_eq!(field(&pass, "active_timeouts"), int(0));
    let active = get(&node, "u", &[])?;
    assert_eq!(field(&active, "state"), text("queued"));
    assert_eq!(field(&active, "terminal_retention_until_ms"), Term::Nil);
    Ok(())
}

#[test]
fn an_overdue_flow_fails_and_starts_its_retention() -> TestResult {
    // Elixir: "max_active_ms moves overdue non-terminal flows to failed and
    // starts retention". The type's retention goes through the text form,
    // as Elixir's typed protocol has none.
    let node = open(&directory("audit-timeout-overdue")?, 4)?;
    let now = ahead()?;
    let policy = ok(
        &node,
        "FLOW.POLICY.SET",
        &[("type", text("overdue")), ("max_active_ms", int(500))],
    )?;
    assert_eq!(field(&policy, "max_active_ms"), int(500));
    let args = [text("overdue"), text("retention_ttl"), int(1_000)];
    let wf_application::commands::flow::text::Parsed::Command(command) =
        wf_application::commands::flow::text::parse(b"FLOW.POLICY.SET", &args)?
    else {
        return Err("not a command".into());
    };
    crate::call(|reply| {
        wf_application::commands::flow::FlowService::execute(&node, command, reply);
    })??;
    let in_p = [("partition_key", text("tenant"))];
    let options = [
        ("partition_key", text("tenant")),
        ("run_at_ms", int(now + 10_000)),
        ("now_ms", int(now)),
    ];
    create(&node, "o", "overdue", &options)?;
    assert_eq!(field(&get(&node, "o", &in_p)?, "max_active_ms"), int(500));
    assert_eq!(
        field(&cleanup(&node, 10, now + 499)?, "active_timeouts"),
        int(0)
    );
    assert_eq!(field(&get(&node, "o", &in_p)?, "state"), text("queued"));
    let pass = cleanup(&node, 10, now + 500)?;
    assert_eq!(
        (field(&pass, "active_timeouts"), field(&pass, "flows")),
        (int(1), int(0))
    );
    let failed = get(
        &node,
        "o",
        &[
            ("partition_key", text("tenant")),
            ("full", Term::Bool(true)),
        ],
    )?;
    assert_eq!(field(&failed, "state"), text("failed"));
    assert_eq!(field(&failed, "max_active_ms"), int(500));
    assert_eq!(field(&failed, "updated_at_ms"), int(now + 500));
    assert_eq!(
        field(&failed, "terminal_retention_until_ms"),
        int(now + 1_500)
    );
    assert_eq!(field(&failed, "lease_owner"), Term::Nil);
    assert_eq!(field(&failed, "lease_token"), Term::Nil);
    assert_eq!(field(&failed, "next_run_at_ms"), Term::Nil);
    assert_eq!(
        super::sorted(&field(&failed, "error")),
        super::sorted(&timed_out(500))
    );
    let history = [("partition_key", text("tenant")), ("count", int(10))];
    let recorded = events(&node, "o", &history)?.into_iter().any(|event| {
        field(&event, "event") == text("failed")
            && field(&event, "reason") == text("max_active_ms")
            && field(&event, "max_active_ms") == text("500")
    });
    assert!(recorded);
    Ok(())
}

#[test]
fn one_cleanup_limit_covers_timeouts_and_removals() -> TestResult {
    // Elixir: "timeout cleanup also removes terminal flows already expired
    // before the pass", "timeout cleanup applies one active candidate limit
    // across shards" and "timeout and terminal cleanup share one limit
    // across shards".
    let node = open(&directory("audit-timeout-limit")?, 4)?;
    let now = ahead()?;
    let in_p = [("partition_key", text("mixed"))];
    let expiring = [
        ("partition_key", text("mixed")),
        ("run_at_ms", int(now)),
        ("retention_ttl_ms", int(10)),
        ("now_ms", int(now)),
    ];
    create(&node, "expired", "mixed", &expiring)?;
    let records = claim_at(&node, "mixed", now, &in_p)?;
    let [lease] = records.as_slice() else {
        return Err("one claim".into());
    };
    ok(
        &node,
        "FLOW.COMPLETE",
        &[
            ("id", text("expired")),
            ("partition_key", text("mixed")),
            ("lease_token", field(lease, "lease_token")),
            ("fencing_token", field(lease, "fencing_token")),
            ("now_ms", int(now + 10)),
        ],
    )?;
    let limited = [
        ("partition_key", text("mixed")),
        ("max_active_ms", int(100)),
        ("run_at_ms", int(now + 1_000)),
        ("now_ms", int(now)),
    ];
    create(&node, "active", "mixed", &limited)?;
    let pass = cleanup(&node, 10, now + 100)?;
    assert_eq!(
        (field(&pass, "active_timeouts"), field(&pass, "flows")),
        (int(1), int(1))
    );
    assert_eq!(get(&node, "expired", &in_p)?, Term::Nil);
    assert_eq!(
        field(&get(&node, "active", &in_p)?, "state"),
        text("failed")
    );

    // Four overdue Flows in two partitions: a limit of 2 fails two.
    let node = open(&directory("audit-timeout-limit-global")?, 4)?;
    let mut ids = Vec::new();
    for partition in ["left", "right"] {
        for index in 1..=2 {
            let id = format!("{partition}-{index}");
            let options = [
                ("partition_key", text(partition)),
                ("max_active_ms", int(100)),
                ("run_at_ms", int(now + 1_000)),
                ("now_ms", int(now)),
            ];
            create(&node, &id, "global", &options)?;
            ids.push((id, partition));
        }
    }
    let pass = cleanup(&node, 2, now + 100)?;
    assert_eq!(
        (field(&pass, "active_timeouts"), field(&pass, "flows")),
        (int(2), int(0))
    );
    let mut failed = 0;
    for (id, partition) in &ids {
        if field(
            &get(&node, id, &[("partition_key", text(partition))])?,
            "state",
        ) == text("failed")
        {
            failed += 1;
        }
    }
    assert_eq!(failed, 2);
    Ok(())
}

#[test]
fn a_create_limit_overrides_or_opts_out_of_the_type_s() -> TestResult {
    // Elixir: "create max_active_ms overrides the type policy for one flow"
    // and "create max_active_ms infinity opts one flow out of the type
    // policy timeout".
    let node = open(&directory("audit-timeout-override")?, 4)?;
    let now = ahead()?;
    ok(
        &node,
        "FLOW.POLICY.SET",
        &[("type", text("long")), ("max_active_ms", int(10_000))],
    )?;
    let short = [
        ("max_active_ms", int(100)),
        ("run_at_ms", int(now)),
        ("now_ms", int(now)),
    ];
    create(&node, "short", "long", &short)?;
    assert_eq!(field(&get(&node, "short", &[])?, "max_active_ms"), int(100));
    assert_eq!(
        field(&cleanup(&node, 10, now + 100)?, "active_timeouts"),
        int(1)
    );
    assert_eq!(field(&get(&node, "short", &[])?, "state"), text("failed"));

    ok(
        &node,
        "FLOW.POLICY.SET",
        &[("type", text("brief")), ("max_active_ms", int(100))],
    )?;
    let forever = [
        ("max_active_ms", text("infinity")),
        ("run_at_ms", int(now)),
        ("now_ms", int(now)),
    ];
    create(&node, "forever", "brief", &forever)?;
    assert_eq!(
        field(&get(&node, "forever", &[])?, "max_active_ms"),
        Term::Nil
    );
    assert_eq!(
        field(&cleanup(&node, 10, now + 100)?, "active_timeouts"),
        int(0)
    );
    assert_eq!(field(&get(&node, "forever", &[])?, "state"), text("queued"));
    Ok(())
}

#[test]
fn a_claim_fails_overdue_flows_and_takes_the_next_eligible_one() -> TestResult {
    // Elixir: "claim_due fails a flow whose max active deadline has
    // elapsed", "claim_due keeps a batch of unexpired max active candidates
    // eligible" and "claim_due skips an expired candidate and claims the
    // next eligible flow".
    let node = open(&directory("audit-timeout-claim")?, 4)?;
    let now = ahead()?;
    let limited = [
        ("max_active_ms", int(100)),
        ("run_at_ms", int(now)),
        ("now_ms", int(now)),
    ];
    create(&node, "late", "claim-late", &limited)?;
    assert_eq!(
        claim_at(&node, "claim-late", now + 100, &[])?,
        Vec::<Term>::new()
    );
    let failed = get(&node, "late", &[("full", Term::Bool(true))])?;
    assert_eq!(field(&failed, "state"), text("failed"));
    assert_eq!(field(&failed, "updated_at_ms"), int(now + 100));
    assert_eq!(
        super::sorted(&field(&failed, "error")),
        super::sorted(&timed_out(100))
    );

    let in_p = [("partition_key", text("batch"))];
    for index in 1..=3 {
        let options = [
            ("partition_key", text("batch")),
            ("max_active_ms", int(1_000)),
            ("run_at_ms", int(now + index)),
            ("now_ms", int(now)),
        ];
        create(&node, &format!("live-{index}"), "claim-live", &options)?;
    }
    let mut batch = in_p.to_vec();
    batch.push(("limit", int(3)));
    let records = claim_at(&node, "claim-live", now + 100, &batch)?;
    assert_eq!(records.len(), 3);
    assert!(
        records
            .iter()
            .all(|record| field(record, "state") == text("running"))
    );

    let next = [("partition_key", text("next"))];
    let options = [
        ("partition_key", text("next")),
        ("max_active_ms", int(100)),
        ("run_at_ms", int(now)),
        ("now_ms", int(now)),
    ];
    create(&node, "expired", "claim-next", &options)?;
    let options = [
        ("partition_key", text("next")),
        ("run_at_ms", int(now + 1)),
        ("now_ms", int(now)),
    ];
    create(&node, "eligible", "claim-next", &options)?;
    let records = claim_at(&node, "claim-next", now + 100, &next)?;
    let [record] = records.as_slice() else {
        return Err(format!("one claim expected, got {records:?}").into());
    };
    assert_eq!(field(record, "id"), text("eligible"));
    assert_eq!(
        field(&get(&node, "expired", &next)?, "state"),
        text("failed")
    );
    Ok(())
}

#[test]
fn a_completion_past_the_limit_fails_the_flow() -> TestResult {
    // Elixir: "complete fails a running flow whose max active deadline has
    // elapsed".
    let node = open(&directory("audit-timeout-complete")?, 4)?;
    let now = ahead()?;
    let limited = [
        ("max_active_ms", int(100)),
        ("run_at_ms", int(now)),
        ("now_ms", int(now)),
    ];
    create(&node, "c", "complete-late", &limited)?;
    let records = claim_at(&node, "complete-late", now, &[("lease_ms", int(1_000))])?;
    let [lease] = records.as_slice() else {
        return Err("one claim".into());
    };
    let late = run(
        &node,
        "FLOW.COMPLETE",
        &[
            ("id", text("c")),
            ("lease_token", field(lease, "lease_token")),
            ("fencing_token", field(lease, "fencing_token")),
            ("now_ms", int(now + 100)),
        ],
    )?;
    assert!(refusal(late)?.contains("max_active_ms"));
    let failed = get(&node, "c", &[("full", Term::Bool(true))])?;
    assert_eq!(field(&failed, "state"), text("failed"));
    assert_eq!(field(&failed, "updated_at_ms"), int(now + 100));
    assert_eq!(
        super::sorted(&field(&failed, "error")),
        super::sorted(&timed_out(100))
    );
    Ok(())
}

#[test]
fn a_fast_path_completion_past_the_limit_fails_the_flow() -> TestResult {
    // The queue protocol's completion (`COMPLETE_MANY` 0x92) meets the same
    // rule as `FLOW.COMPLETE` (audit gap G21); it runs at the server clock,
    // so the Flow is created in the past.
    let node = open(&directory("audit-timeout-fast")?, 4)?;
    let past = ahead()? - 120_000;
    create(
        &node,
        "f",
        "jobs",
        &[
            ("max_active_ms", int(100)),
            ("run_at_ms", int(past)),
            ("now_ms", int(past)),
        ],
    )?;
    let mut request = crate::claim_request(1);
    request.now_ms = Some(u64::try_from(past)?);
    let jobs = crate::call(|reply| {
        wf_application::commands::queue::QueueService::claim(&node, request, reply);
    })??;
    let [job] = jobs.as_slice() else {
        return Err("one job".into());
    };
    let outcomes = crate::complete(&node, vec![crate::completion(job)])?;
    let refused = outcomes
        .first()
        .ok_or("no outcome")?
        .clone()
        .err()
        .ok_or("completed")?;
    assert_eq!(refused.to_string(), "ERR flow max_active_ms exceeded");
    let failed = get(&node, "f", &[])?;
    assert_eq!(field(&failed, "state"), text("failed"));
    Ok(())
}
