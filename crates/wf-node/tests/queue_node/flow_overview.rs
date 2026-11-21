//! `FLOW.GOVERNANCE.OVERVIEW` (ADR-014 step 15), ported from Elixir's
//! governance tests: the overview lists approvals, budgets, limits and
//! circuits across shards with counts, and honours partition scopes.

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::ServiceError;
use wf_node::composition::QueueNode;

use super::{TestResult, call, directory, open};

fn options(entries: &[(&str, Term)]) -> Term {
    Term::Map(
        entries
            .iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value.clone()))
            .collect(),
    )
}

fn run(
    node: &QueueNode,
    name: &str,
    entries: &[(&str, Term)],
) -> TestResult<Result<Term, ServiceError>> {
    match flow::parse(name, &options(entries)) {
        Ok(command) => call(|reply| node.execute(command, reply)),
        Err(error) => Ok(Err(error)),
    }
}

fn field(term: &Term, path: &[&str]) -> Term {
    path.iter()
        .try_fold(term, |term, key| term.get(key.as_bytes()))
        .cloned()
        .unwrap_or(Term::Nil)
}

fn scopes(overview: &Term, kind: &str) -> Vec<Term> {
    match field(overview, &[kind]) {
        Term::List(items) => items.iter().map(|item| field(item, &["scope"])).collect(),
        _ => Vec::new(),
    }
}

/// An approval, a budget, a limit and a circuit under `scope`.
fn records(node: &QueueNode, tag: &str, scope: &str) -> TestResult {
    let scoped = |extra: &[(&'static str, Term)]| {
        let mut entries = vec![("scope", Term::text(scope))];
        entries.extend(extra.iter().cloned());
        entries
    };
    run(
        node,
        "FLOW.APPROVAL.REQUEST",
        &[
            ("id", Term::text(&format!("a-{tag}"))),
            ("flow_id", Term::text(&format!("f-{tag}"))),
            ("scope", Term::text(scope)),
            ("now_ms", Term::Int(1_000)),
        ],
    )??;
    run(
        node,
        "FLOW.BUDGET.RESERVE",
        &scoped(&[
            ("amount", Term::Int(1)),
            ("limit", Term::Int(10)),
            ("window_ms", Term::Int(60_000)),
            ("now_ms", Term::Int(1_000)),
        ]),
    )??;
    run(
        node,
        "FLOW.LIMIT.LEASE",
        &scoped(&[
            ("shard_id", Term::Int(0)),
            ("amount", Term::Int(1)),
            ("limit", Term::Int(2)),
            ("ttl_ms", Term::Int(30_000)),
            ("now_ms", Term::Int(1_000)),
        ]),
    )??;
    run(
        node,
        "FLOW.CIRCUIT.OPEN",
        &scoped(&[("now_ms", Term::Int(1_000))]),
    )??;
    Ok(())
}

#[test]
fn the_overview_summarizes_every_kind_across_shards() -> TestResult {
    let node = open(&directory("flow-overview")?, 4)?;
    for (index, scope) in ["s1", "s2", "s3"].iter().enumerate() {
        records(&node, &index.to_string(), scope)?;
    }
    run(
        &node,
        "FLOW.APPROVAL.APPROVE",
        &[
            ("id", Term::text("a-1")),
            ("approver", Term::text("x")),
            ("now_ms", Term::Int(1_001)),
        ],
    )??;
    run(
        &node,
        "FLOW.CIRCUIT.CLOSE",
        &[("scope", Term::text("s2")), ("now_ms", Term::Int(1_001))],
    )??;
    let overview = run(
        &node,
        "FLOW.GOVERNANCE.OVERVIEW",
        &[("limit", Term::Int(200)), ("now_ms", Term::Int(2_000))],
    )??;
    for (count, expected) in [
        ("approvals", 3),
        ("pending_approvals", 2),
        ("budgets", 3),
        ("limits", 3),
        ("circuits", 3),
        ("open_circuits", 2),
        ("half_open_circuits", 0),
    ] {
        assert_eq!(
            field(&overview, &["counts", count]),
            Term::Int(expected),
            "{count}"
        );
    }
    // Open circuits come first, then by scope.
    assert_eq!(
        scopes(&overview, "circuits"),
        ["s1", "s3", "s2"].map(Term::text).to_vec()
    );
    assert_eq!(
        scopes(&overview, "budgets"),
        ["s1", "s2", "s3"].map(Term::text).to_vec()
    );
    let pending = run(
        &node,
        "FLOW.GOVERNANCE.OVERVIEW",
        &[("status", Term::text("pending"))],
    )??;
    assert_eq!(field(&pending, &["counts", "approvals"]), Term::Int(2));
    assert_eq!(
        run(
            &node,
            "FLOW.GOVERNANCE.OVERVIEW",
            &[("limit", Term::Int(0))]
        )?,
        Err(ServiceError::rejected(
            "ERR flow approval limit must be a positive integer".to_owned()
        ))
    );
    Ok(())
}

#[test]
fn partition_scopes_filter_the_overview() -> TestResult {
    let node = open(&directory("flow-overview-partition")?, 4)?;
    records(&node, "visible", "partition:pv")?;
    records(&node, "hidden", "partition:ph")?;
    let overview = run(
        &node,
        "FLOW.GOVERNANCE.OVERVIEW",
        &[
            ("partition_key", Term::text("pv")),
            ("limit", Term::Int(200)),
        ],
    )??;
    for kind in ["approvals", "budgets", "limits", "circuits"] {
        assert_eq!(
            scopes(&overview, kind),
            vec![Term::text("partition:pv")],
            "{kind}"
        );
    }
    Ok(())
}
