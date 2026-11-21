//! Effects need the Flow's current lease, default to the Flow id's auto
//! partition, and live and go with the Flow: they survive a restart and a
//! snapshot, and retention removes them with it.

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::{QueueService, ServiceError};
use wf_node::composition::QueueNode;

use super::super::{TestResult, call, directory, open};
use super::{claimed, effect, field, kinds, ledger, options, reserve, run, text, with};

fn refused(text: &str) -> Result<Term, ServiceError> {
    Err(ServiceError::rejected(text.to_owned()))
}

#[test]
fn the_current_lease_and_fence_are_required() -> TestResult {
    let node = open(&directory("flow-effect-lease")?, 4)?;
    let lease = claimed(&node, "l1", None)?;
    let bare = [("id", text("l1")), ("partition_key", text("p"))];
    assert_eq!(
        reserve(&node, &bare, &[])?,
        refused("ERR flow lease_token must be a non-empty string")
    );
    assert_eq!(
        reserve(&node, &lease, &[("lease_token", text("other"))])?,
        refused("ERR flow lease token mismatch")
    );
    assert_eq!(
        reserve(&node, &lease, &[("fencing_token", Term::Int(99))])?,
        refused("ERR flow fencing token mismatch")
    );
    // The Flow is looked up before the reservation's own options.
    assert_eq!(
        reserve(
            &node,
            &[("id", text("missing")), ("partition_key", text("p"))],
            &[("operation_digest", Term::Nil)],
        )?,
        refused("ERR flow not found")
    );
    assert_eq!(
        reserve(&node, &lease, &[("operation_digest", Term::Nil)])?,
        refused("ERR flow operation_digest must be a non-empty string of at most 262144 bytes")
    );
    reserve(&node, &lease, &[])??;
    run(
        &node,
        "FLOW.COMPLETE",
        &with(&lease, &[("now_ms", Term::Int(1_010))]),
    )??;
    // A completed Flow holds no lease; its records stay readable.
    assert_eq!(
        reserve(&node, &lease, &[("effect_key", text("late"))])?,
        refused("ERR flow lease token mismatch")
    );
    assert_eq!(
        field(&effect(&node, "l1", "send")?, "status"),
        text("reserved")
    );
    Ok(())
}

#[test]
fn reads_default_to_the_flow_ids_auto_partition() -> TestResult {
    let node = open(&directory("flow-effect-auto")?, 4)?;
    run(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("auto")),
            ("type", text("ta")),
            ("run_at_ms", Term::Int(1_000)),
            ("now_ms", Term::Int(1_000)),
        ],
    )??;
    let claim = flow::claim::parse(
        "FLOW.CLAIM_DUE",
        &options(&[
            ("type", text("ta")),
            ("worker", text("w")),
            ("now_ms", Term::Int(1_001)),
        ]),
    )?;
    let jobs = call(|reply| QueueService::claim(&node, claim.request, reply))??;
    let job = jobs.first().ok_or("nothing claimed")?;
    let lease = [
        ("id", text("auto")),
        ("lease_token", Term::Bin(job.lease_token.clone())),
        ("fencing_token", Term::Int(i64::try_from(job.fence)?)),
    ];
    let reserved = reserve(&node, &lease, &[])??;
    let get = |entries: &[(&str, Term)]| -> TestResult<Term> {
        let command = flow::parse("FLOW.EFFECT.GET", &options(entries))?;
        Ok(call(|reply| node.execute(command, reply))??)
    };
    let fetched = get(&[("id", text("auto")), ("effect_key", text("send"))])?;
    assert_eq!(field(&fetched, "flow_id"), field(&reserved, "flow_id"));
    assert_eq!(field(&fetched, "effect_key"), text("send"));
    let Term::Bin(partition) = field(&fetched, "partition_key") else {
        return Err("no partition".into());
    };
    assert!(partition.starts_with(b"__flow_auto__:"));
    // Another partition names other records.
    let elsewhere = get(&[
        ("id", text("auto")),
        ("effect_key", text("send")),
        ("partition_key", text("p")),
    ])?;
    assert_eq!(elsewhere, Term::Nil);
    Ok(())
}

fn records(node: &QueueNode, id: &str) -> TestResult<(Term, Term)> {
    Ok((effect(node, id, "send")?, ledger(node, id, &[])?))
}

#[test]
fn records_survive_restarts_and_snapshots() -> TestResult {
    let path = directory("flow-effect-restart")?;
    let (before, lease) = {
        let node = open(&path, 4)?;
        let lease = claimed(&node, "r1", None)?;
        reserve(&node, &lease, &[("idempotency_key", Term::Nil)])?.err();
        reserve(&node, &lease, &[])??;
        node.snapshot()?;
        let terminal = with(
            &lease,
            &[
                ("effect_key", text("send")),
                ("latency_ms", Term::Int(7)),
                ("now_ms", Term::Int(1_004)),
            ],
        );
        run(&node, "FLOW.EFFECT.CONFIRM", &terminal)??;
        (records(&node, "r1")?, lease)
    };
    assert_eq!(
        kinds(&before.1),
        vec![
            text("effect_denied"),
            text("effect_reserved"),
            text("effect_confirmed")
        ]
    );
    let node = open(&path, 4)?;
    assert_eq!(records(&node, "r1")?, before);
    // The refusal count was restored too: a new refusal gets a new id.
    reserve(
        &node,
        &lease,
        &[("idempotency_key", Term::Nil), ("effect_key", text("x"))],
    )?
    .err();
    let Term::List(events) = ledger(&node, "r1", &[])? else {
        return Err("ledger is not a list".into());
    };
    let ids: Vec<Term> = events.iter().map(|event| field(event, "id")).collect();
    assert_eq!(ids.len(), 4);
    assert_ne!(ids.first(), ids.last());
    Ok(())
}

#[test]
fn retention_removes_effects_and_ledger_with_the_flow() -> TestResult {
    let node = open(&directory("flow-effect-retention")?, 4)?;
    let lease = claimed(&node, "t1", Some(60_000))?;
    reserve(&node, &lease, &[])??;
    run(
        &node,
        "FLOW.COMPLETE",
        &with(&lease, &[("now_ms", Term::Int(1_003))]),
    )??;
    let record = run(
        &node,
        "FLOW.GET",
        &[("id", text("t1")), ("partition_key", text("p"))],
    )??;
    let Term::Int(until) = field(&record, "terminal_retention_until_ms") else {
        return Err("no retention time".into());
    };
    let cleaned = run(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[("now_ms", Term::Int(until + 1)), ("limit", Term::Int(10))],
    )??;
    assert_eq!(field(&cleaned, "flows"), Term::Int(1));
    assert_eq!(effect(&node, "t1", "send")?, Term::Nil);
    assert_eq!(ledger(&node, "t1", &[])?, Term::List(Vec::new()));
    Ok(())
}
