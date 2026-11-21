//! `flow/policy_command_test.exs`: its client-visible case, the policy size
//! limit (the rest tests Elixir's policy stamping of commands). The bound
//! applies to the node's own encoding of the policy, not Elixir's term size.

use wf_application::commands::flow::Term;

use super::{field, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

/// A policy with `count` states, each with a retry policy.
fn states(count: usize) -> Term {
    Term::Map(
        (0..count)
            .map(|index| {
                let state = format!("state-{index:05}-{}", "s".repeat(40));
                let retry = map(&[
                    ("max_retries", Term::Int(3)),
                    ("exhausted_to", text("failed")),
                ]);
                (state.into_bytes(), map(&[("retry", retry)]))
            })
            .collect(),
    )
}

#[test]
fn a_policy_past_the_size_limit_is_refused_and_one_under_it_is_kept() -> TestResult {
    // Elixir: "policy snapshot size has a deterministic inclusive boundary".
    let node = open(&directory("audit-policy-size")?, 4)?;
    let kept = ok(
        &node,
        "FLOW.POLICY.SET",
        &[("type", text("sized")), ("states", states(1_000))],
    )?;
    assert_eq!(field(&kept, "generation"), Term::Int(1));
    let oversized = run(
        &node,
        "FLOW.POLICY.SET",
        &[("type", text("sized")), ("states", states(4_000))],
    )?;
    assert_eq!(
        refusal(oversized)?,
        "ERR flow policy snapshot exceeds 262144 bytes"
    );
    let stored = ok(&node, "FLOW.POLICY.GET", &[("type", text("sized"))])?;
    assert_eq!(field(&stored, "generation"), Term::Int(1));
    Ok(())
}
