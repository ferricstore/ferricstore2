//! State-level governance in Flow policies (ADR-014 step 15), ported from
//! Elixir's "flow policy stores governance rules and exposes them": a
//! state's governance is normalized, merged by later sets, removed by nil,
//! refused with Elixir's text when invalid, and survives a restart.

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::ServiceError;
use wf_node::composition::QueueNode;

use super::{TestResult, call, directory, open};

fn map(entries: Vec<(&str, Term)>) -> Term {
    Term::Map(
        entries
            .into_iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value))
            .collect(),
    )
}

fn text(value: &str) -> Term {
    Term::text(value)
}

fn run(node: &QueueNode, name: &str, args: &Term) -> TestResult<Result<Term, ServiceError>> {
    match flow::parse(name, args) {
        Ok(command) => call(|reply| node.execute(command, reply)),
        Err(error) => Ok(Err(error)),
    }
}

fn set(node: &QueueNode, governance: Term) -> TestResult<Result<Term, ServiceError>> {
    let states = map(vec![("queued", map(vec![("governance", governance)]))]);
    run(
        node,
        "FLOW.POLICY.SET",
        &map(vec![("type", text("tg")), ("states", states)]),
    )
}

fn governance(node: &QueueNode) -> TestResult<Term> {
    let view = run(
        node,
        "FLOW.POLICY.GET",
        &map(vec![("type", text("tg")), ("state", text("queued"))]),
    )??;
    Ok(view.get(b"governance").cloned().unwrap_or(Term::Nil))
}

fn path(term: &Term, keys: &[&str]) -> Term {
    keys.iter()
        .try_fold(term, |term, key| term.get(key.as_bytes()))
        .cloned()
        .unwrap_or(Term::Nil)
}

#[test]
fn state_governance_is_normalized_merged_and_kept() -> TestResult {
    let dir = directory("flow-policy-governance")?;
    {
        let node = open(&dir, 4)?;
        set(
            &node,
            map(vec![
                ("mode", text("LEDGER")),
                (
                    "effects",
                    map(vec![
                        (
                            "allowed",
                            Term::List(vec![text("email.send"), text("email.send")]),
                        ),
                        ("denied", Term::List(vec![text("stripe.refund")])),
                        ("require_idempotency_key", Term::Bool(true)),
                    ]),
                ),
                (
                    "audit",
                    map(vec![
                        ("effect_events", Term::Bool(true)),
                        ("denials", text("all")),
                    ]),
                ),
                (
                    "limits",
                    map(vec![("running", map(vec![("limit", Term::Int(3))]))]),
                ),
            ]),
        )??;
        let stored = governance(&node)?;
        assert_eq!(path(&stored, &["mode"]), text("ledger"));
        assert_eq!(
            path(&stored, &["effects", "allowed"]),
            Term::List(vec![text("email.send")])
        );
        assert_eq!(path(&stored, &["audit", "denials"]), text("all"));
        // A later set merges into the stored governance.
        set(
            &node,
            map(vec![(
                "limits",
                map(vec![("running", map(vec![("limit", Term::Int(5))]))]),
            )]),
        )??;
        let merged = governance(&node)?;
        assert_eq!(path(&merged, &["limits", "running", "limit"]), Term::Int(5));
        assert_eq!(path(&merged, &["mode"]), text("ledger"));
        assert_eq!(
            set(&node, map(vec![("mode", text("loud"))]))?,
            Err(ServiceError::rejected(
                "ERR invalid flow governance mode".to_owned()
            ))
        );
        assert_eq!(
            set(
                &node,
                map(vec![(
                    "limits",
                    map(vec![
                        ("b", map(vec![("limit", Term::Int(-1))])),
                        ("a", map(Vec::new()))
                    ])
                )])
            )?,
            Err(ServiceError::rejected(
                "ERR flow governance limit must be a non-negative integer".to_owned()
            ))
        );
    }
    let node = open(&dir, 7)?;
    assert_eq!(
        path(&governance(&node)?, &["limits", "running", "limit"]),
        Term::Int(5)
    );
    set(&node, Term::Nil)??;
    assert_eq!(governance(&node)?, Term::Nil);
    Ok(())
}
