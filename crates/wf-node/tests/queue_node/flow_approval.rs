//! `FLOW.APPROVAL.*` (ADR-014 step 15), ported from Elixir's governance
//! tests: a request takes one terminal decision, options are checked before
//! anything changes, an expired request is recorded as expired, listings
//! filter and order across shards, and approvals survive restarts and
//! snapshots.

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

fn text(value: &str) -> Term {
    Term::text(value)
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

fn field(term: &Term, key: &str) -> Term {
    term.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
}

fn request(
    node: &QueueNode,
    id: &str,
    scope: &str,
    extra: &[(&str, Term)],
) -> TestResult<Result<Term, ServiceError>> {
    let mut entries = vec![
        ("id", text(id)),
        ("flow_id", text("flow-1")),
        ("scope", text(scope)),
        ("now_ms", Term::Int(1_000)),
    ];
    for (key, value) in extra {
        entries.retain(|(held, _)| held != key);
        entries.push((key, value.clone()));
    }
    run(node, "FLOW.APPROVAL.REQUEST", &entries)
}

fn get(node: &QueueNode, id: &str) -> TestResult<Term> {
    Ok(run(node, "FLOW.APPROVAL.GET", &[("id", text(id))])??)
}

fn conflict(result: Result<Term, ServiceError>) -> TestResult<Term> {
    match result {
        Err(ServiceError::Governance(denial)) => Ok(denial.payload()),
        other => Err(format!("expected a conflict, got {other:?}").into()),
    }
}

fn refused(text: &str) -> Result<Term, ServiceError> {
    Err(ServiceError::rejected(text.to_owned()))
}

#[test]
fn a_request_takes_one_terminal_decision_and_survives_restarts() -> TestResult {
    let path = directory("flow-approval")?;
    {
        let node = open(&path, 4)?;
        let requested = request(
            &node,
            "a1",
            "p",
            &[
                ("reason", text("refund approval")),
                ("requested_by", text("worker-1")),
            ],
        )??;
        assert_eq!(field(&requested, "status"), text("pending"));
        let duplicate = conflict(request(&node, "a1", "p", &[("now_ms", Term::Int(1_001))])?)?;
        assert_eq!(field(&duplicate, "code"), text("GOVERNANCE_CONFLICT"));
        assert_eq!(
            field(&duplicate, "message"),
            text("Approval already exists")
        );
        node.snapshot()?;
        let approved = run(
            &node,
            "FLOW.APPROVAL.APPROVE",
            &[
                ("id", text("a1")),
                ("approver", text("admin")),
                ("reason", text("looks good")),
                ("now_ms", Term::Int(1_002)),
            ],
        )??;
        assert_eq!(field(&approved, "status"), text("approved"));
        assert_eq!(field(&approved, "decided_by"), text("admin"));
        assert_eq!(field(&approved, "decision_reason"), text("looks good"));
    }
    for shards in [4, 7] {
        let node = open(&path, shards)?;
        assert_eq!(field(&get(&node, "a1")?, "status"), text("approved"));
        let terminal = conflict(run(
            &node,
            "FLOW.APPROVAL.REJECT",
            &[
                ("id", text("a1")),
                ("approver", text("admin-2")),
                ("now_ms", Term::Int(1_003)),
            ],
        )?)?;
        assert_eq!(field(&terminal, "status"), text("approved"));
        assert_eq!(field(&terminal, "approval_id"), text("a1"));
    }
    Ok(())
}

#[test]
fn options_are_checked_before_anything_changes() -> TestResult {
    let node = open(&directory("flow-approval-options")?, 4)?;
    for (extra, expected) in [
        (
            ("reason", Term::Int(123)),
            "ERR flow approval reason must be a non-empty string",
        ),
        (
            ("requested_by", text("")),
            "ERR flow approval requested_by must be a non-empty string",
        ),
        (
            ("assignees", text("finance")),
            "ERR flow approval assignees must be a list of non-empty strings",
        ),
        (
            (
                "assignees",
                Term::List(
                    (1..=1_001)
                        .map(|index| text(&format!("assignee-{index}")))
                        .collect(),
                ),
            ),
            "ERR flow approval assignees must be a list of non-empty strings",
        ),
        (
            ("timeout_ms", Term::Int(-1)),
            "ERR flow approval timeout_ms must be a positive integer",
        ),
        (
            ("expires_at_ms", Term::Int(-1)),
            "ERR flow approval expires_at_ms must be a non-negative integer",
        ),
        (
            ("expires_at_ms", Term::Int(1_000)),
            "ERR flow approval expires_at_ms must be greater than now_ms",
        ),
    ] {
        assert_eq!(request(&node, "v1", "p", &[extra])?, refused(expected));
    }
    assert_eq!(get(&node, "v1")?, Term::Nil);
    let assigned = request(
        &node,
        "v1",
        "p",
        &[(
            "assignees",
            Term::List(vec![text("ann"), text("bob"), text("ann")]),
        )],
    )??;
    assert_eq!(
        field(&assigned, "assignees"),
        Term::List(vec![text("ann"), text("bob")])
    );
    assert_eq!(
        run(
            &node,
            "FLOW.APPROVAL.APPROVE",
            &[
                ("id", text("v1")),
                ("approver", text("admin")),
                ("reason", Term::Int(123))
            ]
        )?,
        refused("ERR flow approval reason must be a non-empty string")
    );
    assert_eq!(
        run(
            &node,
            "FLOW.APPROVAL.APPROVE",
            &[
                ("id", text("v1")),
                ("approver", text("admin")),
                ("now_ms", Term::Int(999))
            ]
        )?,
        refused("ERR flow approval now_ms cannot precede requested_at_ms")
    );
    assert_eq!(field(&get(&node, "v1")?, "status"), text("pending"));
    assert_eq!(
        run(
            &node,
            "FLOW.APPROVAL.APPROVE",
            &[("id", text("none")), ("approver", text("admin"))]
        )?,
        refused("ERR flow approval not found")
    );
    Ok(())
}

#[test]
fn a_decision_after_expiry_records_the_expiry() -> TestResult {
    let node = open(&directory("flow-approval-expiry")?, 4)?;
    let requested = request(&node, "x1", "p", &[("timeout_ms", Term::Int(10_000))])??;
    assert_eq!(field(&requested, "expires_at_ms"), Term::Int(11_000));
    let decide = |now: i64| {
        run(
            &node,
            "FLOW.APPROVAL.APPROVE",
            &[
                ("id", text("x1")),
                ("approver", text("a")),
                ("now_ms", Term::Int(now)),
            ],
        )
    };
    let expired = conflict(decide(11_000)?)?;
    assert_eq!(field(&expired, "status"), text("expired"));
    assert_eq!(
        field(&expired, "message"),
        text("Approval x1 expired before it was decided")
    );
    let stored = get(&node, "x1")?;
    assert_eq!(field(&stored, "status"), text("expired"));
    assert_eq!(field(&stored, "decided_at_ms"), Term::Int(11_000));
    assert_eq!(field(&stored, "decided_by"), Term::Nil);
    let again = conflict(decide(12_000)?)?;
    assert_eq!(
        field(&again, "message"),
        text("Approval x1 already has terminal status expired")
    );
    Ok(())
}

fn ids(listed: &Term) -> Vec<Term> {
    match listed {
        Term::List(items) => items.iter().map(|item| field(item, "id")).collect(),
        _ => Vec::new(),
    }
}

#[test]
fn listings_filter_and_order_across_shards() -> TestResult {
    let node = open(&directory("flow-approval-list")?, 4)?;
    for (index, scope) in ["p", "partition:p", "q", "p", "p"].iter().enumerate() {
        let now = 1_000 + i64::try_from(index)?;
        let flow = if index % 2 == 0 { "f-even" } else { "f-odd" };
        request(
            &node,
            &format!("l{index}"),
            scope,
            &[("now_ms", Term::Int(now)), ("flow_id", text(flow))],
        )??;
    }
    run(
        &node,
        "FLOW.APPROVAL.REJECT",
        &[
            ("id", text("l3")),
            ("approver", text("a")),
            ("now_ms", Term::Int(1_010)),
        ],
    )??;
    let list = |entries: &[(&str, Term)]| -> TestResult<Vec<Term>> {
        Ok(ids(&run(&node, "FLOW.APPROVAL.LIST", entries)??))
    };
    let all = list(&[])?;
    assert_eq!(all, ["l4", "l3", "l2", "l1", "l0"].map(text).to_vec());
    assert_eq!(
        list(&[("scope", text("p"))])?,
        ["l4", "l3", "l0"].map(text).to_vec()
    );
    assert_eq!(
        list(&[("partition_key", text("p"))])?,
        ["l4", "l3", "l1", "l0"].map(text).to_vec()
    );
    assert_eq!(
        list(&[("status", text("pending")), ("scope", text("p"))])?,
        ["l4", "l0"].map(text).to_vec()
    );
    assert_eq!(
        list(&[("flow_id", text("f-odd"))])?,
        ["l3", "l1"].map(text).to_vec()
    );
    assert_eq!(
        list(&[("limit", Term::Int(2))])?,
        ["l4", "l3"].map(text).to_vec()
    );
    assert_eq!(
        run(&node, "FLOW.APPROVAL.LIST", &[("status", text("done"))])?,
        refused("ERR flow approval status must be pending, approved, rejected, or expired")
    );
    Ok(())
}

#[test]
fn a_snapshot_alone_restores_approvals() -> TestResult {
    use wf_node::composition::QueueNodeConfig;
    use wf_storage::log::GroupLogLimits;
    let path = directory("flow-approval-snapshot")?;
    let small = |shards| QueueNodeConfig {
        shards,
        log: GroupLogLimits {
            max_segment_bytes: 4_096,
            ..QueueNodeConfig::default().log
        },
        snapshot_segments: 0,
        ..QueueNodeConfig::default()
    };
    {
        let node = super::open_with(&path, small(4))?;
        let reason = text(&"r".repeat(200));
        for index in 0..60 {
            request(
                &node,
                &format!("s{index}"),
                "p",
                &[("reason", reason.clone())],
            )??;
        }
        node.snapshot()?;
    }
    let node = super::open_with(&path, small(7))?;
    let listed = run(&node, "FLOW.APPROVAL.LIST", &[("limit", Term::Int(1_000))])??;
    assert_eq!(ids(&listed).len(), 60);
    assert_eq!(field(&get(&node, "s59")?, "status"), text("pending"));
    Ok(())
}
