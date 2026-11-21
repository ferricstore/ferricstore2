//! `FLOW.BUDGET.*` (ADR-014 step 15), ported from Elixir's governance
//! tests: a fixed window refuses what would pass its limit and resets once
//! it has passed, reservations settle with what was used, many settle in
//! one scope, unsafe usage and out-of-range totals leave the record as it
//! was, and budgets list by scope and survive restarts.

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::ServiceError;
use wf_node::composition::QueueNode;

use super::{TestResult, call, directory, open};

const MAX_EXACT: i64 = 9_007_199_254_740_991;

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

fn field(term: &Term, key: &str) -> Term {
    term.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
}

fn reserve(
    node: &QueueNode,
    scope: &str,
    amount: i64,
    extra: &[(&str, Term)],
) -> TestResult<Result<Term, ServiceError>> {
    let mut entries = vec![("scope", Term::text(scope)), ("amount", Term::Int(amount))];
    entries.extend(extra.iter().cloned());
    run(node, "FLOW.BUDGET.RESERVE", &entries)
}

fn settle(
    node: &QueueNode,
    name: &str,
    scope: &str,
    id: &str,
    extra: &[(&str, Term)],
) -> TestResult<Result<Term, ServiceError>> {
    let mut entries = vec![
        ("scope", Term::text(scope)),
        ("reservation_id", Term::text(id)),
    ];
    entries.extend(extra.iter().cloned());
    run(node, name, &entries)
}

fn get(node: &QueueNode, scope: &str) -> TestResult<Term> {
    Ok(run(
        node,
        "FLOW.BUDGET.GET",
        &[("scope", Term::text(scope))],
    )??)
}

fn shape(now: i64) -> [(&'static str, Term); 3] {
    [
        ("limit", Term::Int(100)),
        ("window_ms", Term::Int(60_000)),
        ("now_ms", Term::Int(now)),
    ]
}

#[test]
fn a_fixed_window_refuses_past_its_limit_and_resets() -> TestResult {
    let path = directory("flow-budget-window")?;
    {
        let node = open(&path, 4)?;
        assert_eq!(
            field(&reserve(&node, "b1", 40, &shape(1_000))??, "used"),
            Term::Int(40)
        );
        assert_eq!(
            field(
                &reserve(&node, "b1", 60, &[("now_ms", Term::Int(2_000))])??,
                "used"
            ),
            Term::Int(100)
        );
        let Err(ServiceError::Governance(denial)) =
            reserve(&node, "b1", 1, &[("now_ms", Term::Int(3_000))])?
        else {
            return Err("expected an exhausted budget".into());
        };
        let denial = denial.payload();
        assert_eq!(
            field(&denial, "code"),
            Term::text("GOVERNANCE_BUDGET_EXHAUSTED")
        );
        assert_eq!(field(&denial, "reason"), Term::text("budget_exhausted"));
        assert_eq!(field(&denial, "retry_after_ms"), Term::Int(58_000));
        assert_eq!(
            field(
                &reserve(&node, "b1", 1, &[("now_ms", Term::Int(61_000))])??,
                "used"
            ),
            Term::Int(1)
        );
        // A first reservation the new budget cannot hold still creates it.
        assert!(reserve(&node, "b2", 500, &shape(1_000))?.is_err());
        assert_eq!(field(&get(&node, "b2")?, "used"), Term::Int(0));
        assert_eq!(
            reserve(&node, "b3", 1, &[("now_ms", Term::Int(1))])?,
            Err(ServiceError::rejected(
                "ERR flow budget limit must be a positive integer".to_owned()
            ))
        );
        assert_eq!(get(&node, "b3")?, Term::Nil);
    }
    let node = open(&path, 7)?;
    let fetched = get(&node, "b1")?;
    assert_eq!(field(&fetched, "used"), Term::Int(1));
    assert_eq!(field(&fetched, "window_start_ms"), Term::Int(61_000));
    let listed = run(&node, "FLOW.BUDGET.LIST", &[("limit", Term::Int(10))])??;
    let Term::List(budgets) = listed else {
        return Err("listing is not a list".into());
    };
    let scopes: Vec<Term> = budgets
        .iter()
        .map(|budget| field(budget, "scope"))
        .collect();
    assert_eq!(scopes, vec![Term::text("b1"), Term::text("b2")]);
    Ok(())
}

#[test]
fn reservations_settle_with_what_was_used() -> TestResult {
    let node = open(&directory("flow-budget-settle")?, 4)?;
    let mut first = shape(1_000).to_vec();
    first.push(("reservation_id", Term::text("llm-step")));
    let reserved = reserve(&node, "s1", 80, &first)??;
    assert_eq!(field(&reserved, "reservation_id"), Term::text("llm-step"));
    assert_eq!(field(&reserved, "status"), Term::text("reserved"));
    let usage = Term::Map(vec![
        (b"model".to_vec(), Term::text("gpt")),
        (b"tokens".to_vec(), Term::Int(55)),
    ]);
    let committed = settle(
        &node,
        "FLOW.BUDGET.COMMIT",
        "s1",
        "llm-step",
        &[
            ("actual_amount", Term::Int(55)),
            ("usage", usage.clone()),
            ("now_ms", Term::Int(2_000)),
        ],
    )??;
    assert_eq!(field(&committed, "used"), Term::Int(55));
    assert_eq!(field(&committed, "usage"), usage);
    reserve(
        &node,
        "s1",
        30,
        &[
            ("reservation_id", Term::text("unused-step")),
            ("now_ms", Term::Int(2_100)),
        ],
    )??;
    let released = settle(
        &node,
        "FLOW.BUDGET.RELEASE",
        "s1",
        "unused-step",
        &[("now_ms", Term::Int(2_200))],
    )??;
    assert_eq!(field(&released, "used"), Term::Int(55));
    assert_eq!(field(&released, "status"), Term::text("released"));
    assert_eq!(field(&released, "actual_amount"), Term::Int(0));
    // A generated id is 22 URL-safe characters, like Elixir's.
    let generated = reserve(&node, "s1", 1, &[("now_ms", Term::Int(2_300))])??;
    let Term::Bin(id) = field(&generated, "reservation_id") else {
        return Err("no reservation id".into());
    };
    assert_eq!(id.len(), 22);
    Ok(())
}

#[test]
fn many_settle_in_one_scope_keeping_the_newest() -> TestResult {
    let node = open(&directory("flow-budget-many")?, 4)?;
    let usage = Term::Map(vec![(b"tokens".to_vec(), Term::Int(1))]);
    for index in 1..=129 {
        let id = format!("llm-step-{index}");
        let extra = [
            ("limit", Term::Int(1_000)),
            ("window_ms", Term::Int(60_000)),
            ("reservation_id", Term::text(&id)),
            ("now_ms", Term::Int(1_000 + index)),
        ];
        reserve(&node, "m1", 1, &extra)??;
        let commit = [
            ("actual_amount", Term::Int(1)),
            ("usage", usage.clone()),
            ("now_ms", Term::Int(2_000 + index)),
        ];
        assert_eq!(
            field(
                &settle(&node, "FLOW.BUDGET.COMMIT", "m1", &id, &commit)??,
                "status"
            ),
            Term::text("committed")
        );
    }
    let budget = get(&node, "m1")?;
    assert_eq!(field(&budget, "used"), Term::Int(129));
    assert_eq!(field(&budget, "reservations_count"), Term::Int(128));
    let again = [
        ("actual_amount", Term::Int(1)),
        ("now_ms", Term::Int(2_129)),
    ];
    assert_eq!(
        field(
            &settle(&node, "FLOW.BUDGET.COMMIT", "m1", "llm-step-129", &again)??,
            "status"
        ),
        Term::text("committed")
    );
    // The oldest settled reservation was dropped.
    assert_eq!(
        settle(&node, "FLOW.BUDGET.COMMIT", "m1", "llm-step-1", &again)?,
        Err(ServiceError::rejected(
            "ERR flow budget reservation not found".to_owned()
        ))
    );
    Ok(())
}

#[test]
fn refused_commits_leave_the_record_as_it_was() -> TestResult {
    let node = open(&directory("flow-budget-refused")?, 4)?;
    let mut extra = vec![
        ("limit", Term::Int(10)),
        ("window_ms", Term::Int(1_000)),
        ("reservation_id", Term::text("r")),
        ("now_ms", Term::Int(1_000)),
    ];
    reserve(&node, "u1", 1, &extra)??;
    let before = get(&node, "u1")?;
    let deep = Term::List((0..70).map(Term::Int).collect());
    assert_eq!(
        settle(
            &node,
            "FLOW.BUDGET.COMMIT",
            "u1",
            "r",
            &[
                ("actual_amount", Term::Int(1)),
                ("usage", deep),
                ("now_ms", Term::Int(1_001))
            ]
        )?,
        Err(ServiceError::rejected(
            "ERR flow budget usage must be a bounded portable term".to_owned()
        ))
    );
    assert_eq!(get(&node, "u1")?, before);
    extra = vec![
        ("limit", Term::Int(MAX_EXACT)),
        ("window_ms", Term::Int(1_000)),
        ("reservation_id", Term::text("first")),
        ("now_ms", Term::Int(1_000)),
    ];
    reserve(&node, "x1", MAX_EXACT - 1, &extra)??;
    reserve(
        &node,
        "x1",
        1,
        &[
            ("reservation_id", Term::text("second")),
            ("now_ms", Term::Int(1_001)),
        ],
    )??;
    assert_eq!(
        settle(
            &node,
            "FLOW.BUDGET.COMMIT",
            "x1",
            "second",
            &[
                ("actual_amount", Term::Int(MAX_EXACT)),
                ("now_ms", Term::Int(1_002))
            ]
        )?,
        Err(ServiceError::rejected(
            "ERR flow budget aggregate usage exceeds durable integer range".to_owned()
        ))
    );
    assert_eq!(field(&get(&node, "x1")?, "used"), Term::Int(MAX_EXACT));
    assert_eq!(
        reserve(&node, "y1", MAX_EXACT + 1, &shape(1_000))?,
        Err(ServiceError::rejected(
            "ERR flow budget amount must be a positive integer".to_owned()
        ))
    );
    assert_eq!(
        reserve(
            &node,
            "y1",
            1,
            &[("reservation_id", Term::text(&"r".repeat(257)))]
        )?,
        Err(ServiceError::rejected(
            "ERR flow budget reservation_id must be at most 256 bytes".to_owned()
        ))
    );
    assert_eq!(get(&node, "y1")?, Term::Nil);
    Ok(())
}

#[test]
fn earlier_windows_settle_without_moving_usage() -> TestResult {
    let node = open(&directory("flow-budget-carry")?, 4)?;
    let first = [
        ("limit", Term::Int(5)),
        ("window_ms", Term::Int(100)),
        ("reservation_id", Term::text("r7")),
        ("now_ms", Term::Int(1_000)),
    ];
    reserve(&node, "c1", 4, &first)??;
    let committed = settle(
        &node,
        "FLOW.BUDGET.COMMIT",
        "c1",
        "r7",
        &[
            ("actual_amount", Term::Int(6)),
            ("now_ms", Term::Int(1_200)),
        ],
    )??;
    assert_eq!(field(&committed, "used"), Term::Int(0));
    assert_eq!(field(&committed, "overage_amount"), Term::Int(2));
    assert_eq!(field(&committed, "window_start_ms"), Term::Int(1_200));
    reserve(
        &node,
        "c1",
        2,
        &[
            ("reservation_id", Term::text("r8")),
            ("now_ms", Term::Int(1_210)),
        ],
    )??;
    let released = settle(
        &node,
        "FLOW.BUDGET.RELEASE",
        "c1",
        "r8",
        &[("now_ms", Term::Int(1_400))],
    )??;
    assert_eq!(field(&released, "used"), Term::Int(0));
    assert_eq!(field(&released, "window_start_ms"), Term::Int(1_400));
    Ok(())
}
