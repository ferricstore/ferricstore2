//! `FLOW.CIRCUIT.*` (ADR-014 step 15), ported from Elixir's governance
//! tests: a manual open configures and opens a scope's circuit, a close
//! resets it, configured values survive options that omit them, older
//! times and impossible tracking rules are refused before anything is
//! stored, and the event history keeps its bounds.

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

fn field(term: &Term, key: &str) -> Term {
    term.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
}

fn circuit(
    node: &QueueNode,
    name: &str,
    scope: &str,
    extra: &[(&str, Term)],
) -> TestResult<Result<Term, ServiceError>> {
    let mut entries = vec![("scope", Term::text(scope))];
    entries.extend(extra.iter().cloned());
    run(node, name, &entries)
}

fn kinds(view: &Term) -> Vec<Term> {
    match field(view, "events") {
        Term::List(events) => events.iter().map(|event| field(event, "kind")).collect(),
        _ => Vec::new(),
    }
}

fn refused(text: &str) -> Result<Term, ServiceError> {
    Err(ServiceError::rejected(text.to_owned()))
}

#[test]
fn manual_opens_and_closes_keep_the_rule() -> TestResult {
    let path = directory("flow-circuit")?;
    {
        let node = open(&path, 4)?;
        assert_eq!(circuit(&node, "FLOW.CIRCUIT.GET", "c1", &[])??, Term::Nil);
        let opened = circuit(
            &node,
            "FLOW.CIRCUIT.OPEN",
            "c1",
            &[
                ("failure_threshold", Term::Int(9)),
                ("open_ms", Term::Int(4_321)),
                ("now_ms", Term::Int(1_000)),
            ],
        )??;
        assert_eq!(field(&opened, "status"), Term::text("open"));
        assert_eq!(field(&opened, "opened_at_ms"), Term::Int(1_000));
        assert_eq!(
            kinds(&opened),
            vec![Term::text("manual_open"), Term::text("opened")]
        );
        // Options left out keep the configured rule.
        let again = circuit(
            &node,
            "FLOW.CIRCUIT.OPEN",
            "c1",
            &[("now_ms", Term::Int(1_001))],
        )??;
        assert_eq!(field(&again, "failure_threshold"), Term::Int(9));
        assert_eq!(field(&again, "open_ms"), Term::Int(4_321));
        assert_eq!(
            circuit(
                &node,
                "FLOW.CIRCUIT.CLOSE",
                "c1",
                &[("now_ms", Term::Int(999))]
            )?,
            refused("ERR flow circuit now_ms cannot precede updated_at_ms")
        );
        let closed = circuit(
            &node,
            "FLOW.CIRCUIT.CLOSE",
            "c1",
            &[("now_ms", Term::Int(1_002))],
        )??;
        assert_eq!(field(&closed, "status"), Term::text("closed"));
        assert_eq!(field(&closed, "opened_at_ms"), Term::Nil);
        assert_eq!(field(&closed, "last_success_ms"), Term::Int(1_002));
        assert_eq!(field(&closed, "event_count"), Term::Int(6));
        // Closing an unknown scope stores a closed circuit with a threshold
        // of one.
        let fresh = circuit(
            &node,
            "FLOW.CIRCUIT.CLOSE",
            "c2",
            &[("now_ms", Term::Int(5))],
        )??;
        assert_eq!(field(&fresh, "failure_threshold"), Term::Int(1));
        assert_eq!(field(&fresh, "open_ms"), Term::Int(30_000));
    }
    let node = open(&path, 7)?;
    let stored = circuit(&node, "FLOW.CIRCUIT.GET", "c1", &[])??;
    assert_eq!(field(&stored, "status"), Term::text("closed"));
    assert_eq!(field(&stored, "updated_at_ms"), Term::Int(1_002));
    assert_eq!(
        field(&circuit(&node, "FLOW.CIRCUIT.GET", "c2", &[])??, "status"),
        Term::text("closed")
    );
    Ok(())
}

#[test]
fn impossible_rules_are_refused_before_anything_is_stored() -> TestResult {
    let node = open(&directory("flow-circuit-rules")?, 4)?;
    let many: Vec<Term> = (1..=1_001)
        .map(|index| Term::text(&format!("Error{index}")))
        .collect();
    for (extra, expected) in [
        (
            vec![("failure_threshold", Term::Int(65))],
            "ERR flow circuit failure_threshold must be <= 64 without failure_rate_pct",
        ),
        (
            vec![
                ("failure_threshold", Term::Int(65)),
                ("failure_rate_pct", Term::Int(10)),
            ],
            "ERR flow circuit min_calls is required when failure_threshold exceeds 64",
        ),
        (
            vec![("min_calls", Term::Int(65))],
            "ERR flow circuit min_calls must be <= 64",
        ),
        (
            vec![("failure_rate_pct", Term::Int(101))],
            "ERR flow circuit failure_rate_pct must be an integer from 1 to 100",
        ),
        (
            vec![("error_classes", Term::List(many))],
            "ERR flow circuit error_classes must contain at most 1000 values",
        ),
        (
            vec![(
                "error_classes",
                Term::List(vec![Term::text(&"e".repeat(257))]),
            )],
            "ERR flow circuit error_classes must contain non-empty strings of at most 256 bytes",
        ),
        (
            vec![("open_ms", Term::Int(0))],
            "ERR flow circuit open_ms must be a positive integer",
        ),
    ] {
        assert_eq!(
            circuit(&node, "FLOW.CIRCUIT.OPEN", "r1", &extra)?,
            refused(expected)
        );
    }
    assert_eq!(circuit(&node, "FLOW.CIRCUIT.GET", "r1", &[])??, Term::Nil);
    let wide = circuit(
        &node,
        "FLOW.CIRCUIT.OPEN",
        "r1",
        &[
            ("failure_threshold", Term::Int(65)),
            ("failure_rate_pct", Term::Int(10)),
            ("min_calls", Term::Int(10)),
        ],
    )??;
    assert_eq!(field(&wide, "failure_threshold"), Term::Int(65));
    let classes = circuit(
        &node,
        "FLOW.CIRCUIT.OPEN",
        "r2",
        &[(
            "error_classes",
            Term::List(vec![
                Term::text("timeout"),
                Term::text("5xx"),
                Term::text("timeout"),
            ]),
        )],
    )??;
    assert_eq!(
        field(&classes, "error_classes"),
        Term::List(vec![Term::text("timeout"), Term::text("5xx")])
    );
    Ok(())
}

#[test]
fn the_event_history_keeps_its_bounds() -> TestResult {
    let node = open(&directory("flow-circuit-events")?, 4)?;
    for now in 0..35 {
        circuit(
            &node,
            "FLOW.CIRCUIT.OPEN",
            "e1",
            &[("now_ms", Term::Int(4_000 + now))],
        )??;
    }
    let capped = circuit(&node, "FLOW.CIRCUIT.GET", "e1", &[])??;
    assert_eq!(field(&capped, "event_count"), Term::Int(64));
    let Term::List(events) = field(&capped, "events") else {
        return Err("no events".into());
    };
    assert_eq!(
        events.first().map(|event| field(event, "at_ms")),
        Some(Term::Int(4_034))
    );
    // A window keeps only the events within it.
    circuit(
        &node,
        "FLOW.CIRCUIT.OPEN",
        "e2",
        &[("window_ms", Term::Int(50)), ("now_ms", Term::Int(3_000))],
    )??;
    let later = circuit(
        &node,
        "FLOW.CIRCUIT.OPEN",
        "e2",
        &[("now_ms", Term::Int(3_100))],
    )??;
    assert_eq!(field(&later, "event_count"), Term::Int(2));
    Ok(())
}
