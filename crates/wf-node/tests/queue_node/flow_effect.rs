//! `FLOW.EFFECT.*` and `FLOW.GOVERNANCE.LEDGER` (ADR-014 step 14), ported
//! from Elixir's governance tests: a reservation replays for its digest and
//! conflicts for another, a retry is answered from the stored decision,
//! terminal moves happen once, refused options and older times leave the
//! effect as it was, refusals are structured and recorded, the lease is
//! required, and the records follow the Flow through restarts, snapshots
//! and retention.

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::{QueueService, ServiceError};
use wf_node::composition::QueueNode;

use super::{TestResult, call, directory, open};

mod lease;
mod policy_hash;

pub(super) fn options(entries: &[(&str, Term)]) -> Term {
    Term::Map(
        entries
            .iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value.clone()))
            .collect(),
    )
}

pub(super) fn text(value: &str) -> Term {
    Term::text(value)
}

pub(super) fn run(
    node: &QueueNode,
    name: &str,
    entries: &[(&str, Term)],
) -> TestResult<Result<Term, ServiceError>> {
    // A refusal at parse time reaches a client like any other.
    match flow::parse(name, &options(entries)) {
        Ok(command) => call(|reply| node.execute(command, reply)),
        Err(error) => Ok(Err(error)),
    }
}

pub(super) fn field(term: &Term, key: &str) -> Term {
    term.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
}

pub(super) fn kinds(events: &Term) -> Vec<Term> {
    match events {
        Term::List(events) => events.iter().map(|event| field(event, "kind")).collect(),
        _ => Vec::new(),
    }
}

/// A claimed Flow `id` in partition `p` and its lease options.
pub(super) fn claimed(
    node: &QueueNode,
    id: &str,
    retention: Option<i64>,
) -> TestResult<Vec<(&'static str, Term)>> {
    let mut create = vec![
        ("id", text(id)),
        ("type", text("te")),
        ("partition_key", text("p")),
        ("run_at_ms", Term::Int(1_000)),
        ("now_ms", Term::Int(1_000)),
    ];
    create.extend(retention.map(|ttl| ("retention_ttl_ms", Term::Int(ttl))));
    run(node, "FLOW.CREATE", &create)??;
    let claim = flow::claim::parse(
        "FLOW.CLAIM_DUE",
        &options(&[
            ("type", text("te")),
            ("worker", text("w")),
            ("partition_key", text("p")),
            ("now_ms", Term::Int(1_001)),
        ]),
    )?;
    let jobs = call(|reply| QueueService::claim(node, claim.request, reply))??;
    let job = jobs.first().ok_or("nothing claimed")?;
    Ok(vec![
        ("id", text(id)),
        ("partition_key", text("p")),
        ("lease_token", Term::Bin(job.lease_token.clone())),
        ("fencing_token", Term::Int(i64::try_from(job.fence)?)),
    ])
}

pub(super) fn with(
    base: &[(&'static str, Term)],
    extra: &[(&'static str, Term)],
) -> Vec<(&'static str, Term)> {
    let mut entries = base.to_vec();
    entries.extend(extra.iter().cloned());
    entries
}

pub(super) fn reserve(
    node: &QueueNode,
    lease: &[(&'static str, Term)],
    extra: &[(&'static str, Term)],
) -> TestResult<Result<Term, ServiceError>> {
    let base = with(
        lease,
        &[
            ("effect_key", text("send")),
            ("effect_type", text("email.send")),
            ("operation_digest", text("digest-1")),
            ("idempotency_key", text("idem-1")),
            ("now_ms", Term::Int(1_002)),
        ],
    );
    // Later entries win, as a client's options override the defaults.
    let mut entries: Vec<(&str, Term)> = Vec::new();
    for (key, value) in base.into_iter().chain(extra.iter().cloned()) {
        entries.retain(|(held, _)| *held != key);
        entries.push((key, value));
    }
    run(node, "FLOW.EFFECT.RESERVE", &entries)
}

pub(super) fn effect(node: &QueueNode, id: &str, key: &str) -> TestResult<Term> {
    Ok(run(
        node,
        "FLOW.EFFECT.GET",
        &[
            ("id", text(id)),
            ("partition_key", text("p")),
            ("effect_key", text(key)),
        ],
    )??)
}

pub(super) fn ledger(
    node: &QueueNode,
    id: &str,
    extra: &[(&'static str, Term)],
) -> TestResult<Term> {
    Ok(run(
        node,
        "FLOW.GOVERNANCE.LEDGER",
        &with(&[("id", text(id)), ("partition_key", text("p"))], extra),
    )??)
}

fn denial(result: Result<Term, ServiceError>) -> TestResult<Term> {
    match result {
        Err(ServiceError::Governance(denial)) => Ok(denial.payload()),
        other => Err(format!("expected a governance refusal, got {other:?}").into()),
    }
}

#[test]
fn reservations_replay_for_their_digest_and_conflict_for_another() -> TestResult {
    let node = open(&directory("flow-effect-replay")?, 4)?;
    let lease = claimed(&node, "e1", None)?;
    let reserved = reserve(&node, &lease, &[])??;
    assert_eq!(field(&reserved, "status"), text("reserved"));
    assert_eq!(field(&reserved, "decision"), text("reserved"));
    assert_ne!(field(&reserved, "policy_hash"), Term::Nil);
    // A retry is answered from the stored decision, before the policy that
    // would now refuse it for its missing key.
    let replay = reserve(
        &node,
        &lease,
        &[("idempotency_key", Term::Nil), ("now_ms", Term::Int(1_009))],
    )??;
    assert_eq!(field(&replay, "decision"), text("already_reserved"));
    assert_eq!(field(&replay, "created_at_ms"), Term::Int(1_002));
    let conflict = denial(reserve(
        &node,
        &lease,
        &[("operation_digest", text("digest-2"))],
    )?)?;
    assert_eq!(field(&conflict, "code"), text("GOVERNANCE_CONFLICT"));
    assert_eq!(field(&conflict, "policy"), text("effect_idempotency"));
    assert_eq!(field(&conflict, "reason"), text("governance_conflict"));
    assert_eq!(field(&conflict, "status"), text("reserved"));
    // Neither the replay nor the conflict is recorded.
    assert_eq!(
        kinds(&ledger(&node, "e1", &[])?),
        vec![text("effect_reserved")]
    );
    Ok(())
}

#[test]
fn terminal_moves_are_recorded_once_and_listed_by_window() -> TestResult {
    let node = open(&directory("flow-effect-confirm")?, 4)?;
    let lease = claimed(&node, "e2", None)?;
    reserve(&node, &lease, &[])??;
    let terminal = with(
        &lease,
        &[
            ("effect_key", text("send")),
            ("external_id", text("ch_123")),
            ("now_ms", Term::Int(1_003)),
        ],
    );
    let confirmed = run(&node, "FLOW.EFFECT.CONFIRM", &terminal)??;
    assert_eq!(field(&confirmed, "status"), text("confirmed"));
    assert_eq!(field(&confirmed, "decision"), text("confirmed"));
    let again = run(&node, "FLOW.EFFECT.CONFIRM", &terminal)??;
    assert_eq!(field(&again, "decision"), text("already_applied"));
    let stored = effect(&node, "e2", "send")?;
    assert_eq!(field(&stored, "status"), text("confirmed"));
    assert_eq!(field(&stored, "external_id"), text("ch_123"));
    let events = ledger(&node, "e2", &[])?;
    assert_eq!(
        kinds(&events),
        vec![text("effect_reserved"), text("effect_confirmed")]
    );
    let latest = ledger(
        &node,
        "e2",
        &[
            ("rev", Term::Bool(true)),
            ("limit", Term::Int(1)),
            ("from_ms", Term::Int(1_003)),
        ],
    )?;
    assert_eq!(kinds(&latest), vec![text("effect_confirmed")]);
    let newest = ledger(
        &node,
        "e2",
        &[("rev", Term::Bool(true)), ("limit", Term::Int(1))],
    )?;
    assert_eq!(kinds(&newest), vec![text("effect_confirmed")]);
    let oldest = ledger(&node, "e2", &[("to_ms", Term::Int(1_002))])?;
    assert_eq!(kinds(&oldest), vec![text("effect_reserved")]);
    // Another terminal status is refused once one is reached.
    let failed = denial(run(
        &node,
        "FLOW.EFFECT.FAIL",
        &with(&lease, &[("effect_key", text("send"))]),
    )?)?;
    assert_eq!(field(&failed, "policy"), text("effect_lifecycle"));
    assert_eq!(field(&failed, "current_status"), text("confirmed"));
    assert_eq!(field(&failed, "requested_status"), text("failed"));
    Ok(())
}

#[test]
fn refused_options_and_older_times_leave_the_effect_unchanged() -> TestResult {
    let node = open(&directory("flow-effect-unchanged")?, 4)?;
    let lease = claimed(&node, "e3", None)?;
    reserve(&node, &lease, &[])??;
    let key = ("effect_key", text("send"));
    assert_eq!(
        run(
            &node,
            "FLOW.EFFECT.CONFIRM",
            &with(&lease, &[key.clone(), ("external_id", Term::Int(42))])
        )?,
        Err(ServiceError::rejected(
            "ERR flow external_id must be a string".to_owned()
        ))
    );
    assert_eq!(
        run(
            &node,
            "FLOW.EFFECT.CONFIRM",
            &with(&lease, &[key.clone(), ("now_ms", Term::Int(1_001))])
        )?,
        Err(ServiceError::rejected(
            "ERR flow effect now_ms cannot precede updated_at_ms".to_owned()
        ))
    );
    let stored = effect(&node, "e3", "send")?;
    assert_eq!(field(&stored, "status"), text("reserved"));
    assert_eq!(field(&stored, "external_id"), Term::Nil);
    assert_eq!(field(&stored, "updated_at_ms"), Term::Int(1_002));
    assert_eq!(
        run(
            &node,
            "FLOW.EFFECT.COMPENSATE",
            &with(&lease, &[("effect_key", text("ghost"))])
        )?,
        Err(ServiceError::rejected(
            "ERR flow effect not found".to_owned()
        ))
    );
    Ok(())
}

#[test]
fn refusals_are_structured_and_each_recorded() -> TestResult {
    let node = open(&directory("flow-effect-denied")?, 4)?;
    let lease = claimed(&node, "e4", None)?;
    for now in [1_002, 1_003] {
        let refused = denial(reserve(
            &node,
            &lease,
            &[("idempotency_key", Term::Nil), ("now_ms", Term::Int(now))],
        )?)?;
        assert_eq!(field(&refused, "code"), text("GOVERNANCE_EFFECT_DENIED"));
        assert_eq!(field(&refused, "reason"), text("effect_denied"));
        assert_eq!(field(&refused, "policy"), text("idempotency_key_required"));
        assert_eq!(field(&refused, "scope"), text("p"));
        assert_eq!(field(&refused, "type"), text("te"));
        assert_eq!(field(&refused, "state"), text("running"));
        assert_eq!(field(&refused, "effect_type"), text("email.send"));
        assert_ne!(field(&refused, "decision_id"), Term::Nil);
    }
    let scoped = denial(reserve(
        &node,
        &lease,
        &[
            ("idempotency_key", text("")),
            ("governance_scope", text("tenant-1")),
        ],
    )?)?;
    assert_eq!(field(&scoped, "scope"), text("tenant-1"));
    let Term::List(events) = ledger(&node, "e4", &[])? else {
        return Err("ledger is not a list".into());
    };
    assert_eq!(events.len(), 3);
    let mut seen = std::collections::BTreeSet::new();
    for event in &events {
        assert_eq!(field(event, "kind"), text("effect_denied"));
        assert_eq!(field(event, "status"), text("denied"));
        assert_eq!(field(event, "code"), text("GOVERNANCE_EFFECT_DENIED"));
        let Term::Bin(id) = field(event, "id") else {
            return Err("event id is not a binary".into());
        };
        assert!(seen.insert(id), "event ids repeat");
    }
    assert_eq!(effect(&node, "e4", "send")?, Term::Nil);
    Ok(())
}
