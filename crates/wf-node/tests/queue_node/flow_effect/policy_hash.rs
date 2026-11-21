//! An effect's `policy_hash` names the policy of the Flow's type, as
//! Elixir's does: a type without a policy has the default's hash, a stored
//! policy changes it, a refusal and its ledger event carry the same one,
//! a reserved effect keeps the hash it was decided under, and the hash
//! survives a restart. Unlike Elixir's, it covers the policy's content
//! only, in a canonical order, so equal policies share a hash.

use wf_application::commands::flow::{self, Term};
use wf_application::commands::queue::{QueueService, ServiceError};
use wf_node::composition::QueueNode;

use super::super::{TestResult, call, directory, open};
use super::{effect, field, ledger, options, reserve, run, text};

/// A claimed Flow `id` of `flow_type` in partition `p`, as lease options.
fn claimed_as(
    node: &QueueNode,
    id: &str,
    flow_type: &str,
) -> TestResult<Vec<(&'static str, Term)>> {
    run(
        node,
        "FLOW.CREATE",
        &[
            ("id", text(id)),
            ("type", text(flow_type)),
            ("partition_key", text("p")),
            ("run_at_ms", Term::Int(1_000)),
            ("now_ms", Term::Int(1_000)),
        ],
    )??;
    let claim = flow::claim::parse(
        "FLOW.CLAIM_DUE",
        &options(&[
            ("type", text(flow_type)),
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

/// The hash a reservation on a fresh Flow of `flow_type` is decided under.
fn hash_of(node: &QueueNode, id: &str, flow_type: &str) -> TestResult<Term> {
    let lease = claimed_as(node, id, flow_type)?;
    Ok(field(&reserve(node, &lease, &[])??, "policy_hash"))
}

/// Running limits `a` and `b` on `queued`, the names in the given order.
fn limits(names: [(&str, i64); 2]) -> Term {
    let rules = names
        .iter()
        .map(|(name, limit)| (*name, options(&[("limit", Term::Int(*limit))])))
        .collect::<Vec<_>>();
    options(&[(
        "queued",
        options(&[("governance", options(&[("limits", options(&rules))]))]),
    )])
}

fn set(node: &QueueNode, flow_type: &str, states: Term) -> TestResult {
    run(
        node,
        "FLOW.POLICY.SET",
        &[("type", text(flow_type)), ("states", states)],
    )??;
    Ok(())
}

#[test]
fn the_hash_names_the_types_policy_and_follows_its_changes() -> TestResult {
    let path = directory("flow-effect-policy-hash")?;
    let changed = {
        let node = open(&path, 4)?;
        let default = hash_of(&node, "plain-1", "plain")?;
        assert_eq!(hash_of(&node, "plain-2", "other")?, default);
        set(&node, "ta", limits([("a", 1), ("b", 2)]))?;
        set(&node, "tb", limits([("b", 2), ("a", 1)]))?;
        let first = hash_of(&node, "a-1", "ta")?;
        assert_ne!(first, default);
        assert_eq!(hash_of(&node, "b-1", "tb")?, first);
        // A refusal and its ledger event carry the type's hash.
        let lease = claimed_as(&node, "a-2", "ta")?;
        let Err(ServiceError::Governance(denial)) =
            reserve(&node, &lease, &[("idempotency_key", Term::Nil)])?
        else {
            return Err("expected a refusal".into());
        };
        assert_eq!(field(&denial.payload(), "policy_hash"), first);
        let Term::List(events) = ledger(&node, "a-2", &[])? else {
            return Err("expected the ledger".into());
        };
        let hashes: Vec<Term> = events
            .iter()
            .map(|event| field(event, "policy_hash"))
            .collect();
        assert_eq!(hashes, vec![first.clone()]);
        // A new policy changes the hash of new decisions only.
        set(&node, "ta", limits([("a", 3), ("b", 2)]))?;
        let changed = hash_of(&node, "a-3", "ta")?;
        assert_ne!(changed, first);
        assert_eq!(field(&effect(&node, "a-1", "send")?, "policy_hash"), first);
        changed
    };
    let node = open(&path, 4)?;
    assert_eq!(hash_of(&node, "a-4", "ta")?, changed);
    Ok(())
}
