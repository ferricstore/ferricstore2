//! `flow/history_query_test.exs` and `flow/history_api_test.exs`, as
//! `FLOW.HISTORY` takes them: ranges run forward, event ids within one
//! millisecond compare by version number, cursors must be event ids, and
//! value hydration stays within its ceiling. (Their filter matching is in
//! `audit::history_filters`; scan counts, keyword lists and configured
//! limits are Elixir's.)

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{field, int, ok, run, text};
use crate::{TestResult, directory, open};

const P: &str = "p";

fn history(node: &QueueNode, extra: &[(&str, Term)]) -> TestResult<Result<Vec<Term>, String>> {
    let mut entries = vec![("id", text("flow-1")), ("partition_key", text(P))];
    entries.extend(extra.iter().cloned());
    Ok(match run(node, "FLOW.HISTORY", &entries)? {
        Ok(Term::List(events)) => Ok(events
            .iter()
            .map(|event| match event {
                Term::List(pair) => pair.first().cloned().unwrap_or(Term::Nil),
                other => field(other, "event_id"),
            })
            .collect()),
        Ok(other) => Err(format!("events expected, got {other:?}")),
        Err(error) => Err(error.to_string()),
    })
}

/// `flow-1`, created and moved ten times, all at 1000: event ids `1000-1`
/// to `1000-11`.
fn flow_with_eleven_versions(node: &QueueNode) -> TestResult {
    ok(
        node,
        "FLOW.CREATE",
        &[
            ("id", text("flow-1")),
            ("type", text("history")),
            ("state", text("s0")),
            ("partition_key", text(P)),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
        ],
    )?;
    for step in 1..=10 {
        ok(
            node,
            "FLOW.TRANSITION",
            &[
                ("id", text("flow-1")),
                ("partition_key", text(P)),
                ("from_state", text(&format!("s{}", step - 1))),
                ("to_state", text(&format!("s{step}"))),
                ("fencing_token", int(0)),
                ("now_ms", int(1_000)),
            ],
        )?;
    }
    Ok(())
}

fn ids(versions: &[i64]) -> Vec<Term> {
    versions
        .iter()
        .map(|version| text(&format!("1000-{version}")))
        .collect()
}

#[test]
fn event_ids_within_a_millisecond_compare_by_version_number() -> TestResult {
    // Elixir: "event id ranges compare versions within a millisecond as
    // numbers" and "apply reverses before taking count for reverse
    // queries".
    let node = open(&directory("audit-history-ids")?, 4)?;
    flow_with_eleven_versions(&node)?;
    let count = ("count", int(20));
    let from = history(&node, &[("from_event", text("1000-9")), count.clone()])??;
    assert_eq!(from, ids(&[9, 10, 11]));
    let to = history(&node, &[("to_event", text("1000-10")), count.clone()])??;
    assert_eq!(to, ids(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]));
    let newest = history(&node, &[("rev", Term::Bool(true)), ("count", int(2))])??;
    assert_eq!(newest, ids(&[11, 10]));
    let window = [
        ("from_event", text("1000-9")),
        ("to_event", text("1000-10")),
    ];
    assert_eq!(history(&node, &window)??, ids(&[9, 10]));
    Ok(())
}

#[test]
fn history_ranges_run_forward() -> TestResult {
    // Elixir: "range validators reject inverted ranges" and "validates
    // history time range before reading storage".
    let node = open(&directory("audit-history-ranges")?, 4)?;
    flow_with_eleven_versions(&node)?;
    for (range, refused) in [
        (
            [("from_ms", int(10)), ("to_ms", int(1))],
            "ERR flow from_ms must be <= to_ms",
        ),
        (
            [("from_version", int(3)), ("to_version", int(2))],
            "ERR flow from_version must be <= to_version",
        ),
        (
            [("from_event", text("2000-2")), ("to_event", text("1000-1"))],
            "ERR flow from_event must be <= to_event",
        ),
        (
            [
                ("from_event", text("1000-10")),
                ("to_event", text("1000-9")),
            ],
            "ERR flow from_event must be <= to_event",
        ),
    ] {
        assert_eq!(
            history(&node, &range)?,
            Err(refused.to_owned()),
            "{refused}"
        );
    }
    Ok(())
}

#[test]
fn history_cursors_are_event_ids_and_values_stay_bounded() -> TestResult {
    // Elixir: "prepare rejects malformed history event cursors" and
    // "rejects value hydration above the configured ceiling before reading
    // storage".
    let node = open(&directory("audit-history-cursors")?, 4)?;
    flow_with_eleven_versions(&node)?;
    let from = history(&node, &[("from_event", text("not-an-event"))])?;
    assert_eq!(
        from,
        Err("ERR flow from_event must be a history event id".to_owned())
    );
    let to = history(&node, &[("to_event", text("01-2"))])?;
    assert_eq!(
        to,
        Err("ERR flow to_event must be a history event id".to_owned())
    );
    let window = [("from_event", text("10-2")), ("to_event", text("11-3"))];
    assert_eq!(history(&node, &window)??, Vec::<Term>::new());
    let values = [
        ("values", Term::Bool(true)),
        ("payload_max_bytes", int(65_537)),
    ];
    assert_eq!(
        history(&node, &values)?,
        Err("ERR flow payload_max_bytes exceeds maximum 65536".to_owned())
    );
    Ok(())
}

#[test]
fn history_values_carry_their_size_and_are_omitted_above_the_bound() -> TestResult {
    // Elixir's history value hydration (`HistoryValues.apply_value_result`,
    // audit gap G36): each value comes with its stored size, and one above
    // `payload_max_bytes` is omitted, its size still given.
    let node = open(&directory("audit-history-value-bound")?, 4)?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("flow-1")),
            ("type", text("history")),
            ("partition_key", text(P)),
            ("payload", text("hello world")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
        ],
    )?;
    let created = |max_bytes: Option<i64>| -> TestResult<Term> {
        let mut entries = vec![
            ("id", text("flow-1")),
            ("partition_key", text(P)),
            ("values", Term::Bool(true)),
        ];
        entries.extend(max_bytes.map(|bytes| ("payload_max_bytes", int(bytes))));
        let events = ok(&node, "FLOW.HISTORY", &entries)?;
        let Term::List(events) = events else {
            return Err("events expected".into());
        };
        match events.first() {
            Some(Term::List(pair)) => Ok(pair.get(1).cloned().unwrap_or(Term::Nil)),
            other => Err(format!("an event expected, got {other:?}").into()),
        }
    };
    let whole = created(None)?;
    assert_eq!(field(&whole, "payload"), text("hello world"));
    let Term::Int(size) = field(&whole, "payload_size") else {
        return Err(format!("payload_size expected in {whole:?}").into());
    };
    assert!(size > 0);
    let bounded = created(Some(size - 1))?;
    assert_eq!(field(&bounded, "payload"), Term::Nil);
    assert_eq!(field(&bounded, "payload_omitted"), Term::Bool(true));
    assert_eq!(field(&bounded, "payload_size"), int(size));
    Ok(())
}

#[test]
fn a_value_an_event_still_names_stays_readable_in_history() -> TestResult {
    // The node's own check beside Elixir's history value tests: a payload
    // an event in history names stays readable while named values churn
    // (Elixir releases a value only once no history event names it).
    let node = open(&directory("audit-history-value-kept")?, 4)?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("flow-1")),
            ("type", text("history")),
            ("state", text("s0")),
            ("partition_key", text(P)),
            ("payload", text("first payload")),
            ("values", super::map(&[("a", int(0))])),
            ("history_max_events", int(3)),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
        ],
    )?;
    let step = |at: i64, extra: &[(&str, Term)]| -> TestResult {
        let mut entries = vec![
            ("id", text("flow-1")),
            ("partition_key", text(P)),
            ("from_state", text(&format!("s{}", at - 1))),
            ("to_state", text(&format!("s{at}"))),
            ("fencing_token", int(0)),
            ("now_ms", int(1_000 + at)),
        ];
        entries.extend(extra.iter().cloned());
        ok(&node, "FLOW.TRANSITION", &entries)?;
        Ok(())
    };
    for at in 1..=3 {
        step(
            at,
            &[
                ("values", super::map(&[("a", int(at))])),
                ("override_values", Term::List(vec![text("a")])),
            ],
        )?;
    }
    step(4, &[("payload", text("second payload"))])?;
    step(
        5,
        &[
            ("values", super::map(&[("a", int(9))])),
            ("override_values", Term::List(vec![text("a")])),
        ],
    )?;
    let events = ok(
        &node,
        "FLOW.HISTORY",
        &[
            ("id", text("flow-1")),
            ("partition_key", text(P)),
            ("values", Term::Bool(true)),
        ],
    )?;
    let Term::List(events) = events else {
        return Err("events expected".into());
    };
    let first = events
        .iter()
        .filter_map(|event| match event {
            Term::List(pair) => pair.get(1).cloned(),
            _ => None,
        })
        .find(|fields| field(fields, "version") == int(4) || field(fields, "version") == text("4"))
        .ok_or("the version 4 event")?;
    assert_eq!(field(&first, "payload"), text("first payload"), "{first:?}");
    Ok(())
}

#[test]
fn replaced_values_stay_readable_while_the_flow_lives() -> TestResult {
    // Elixir keeps a replaced payload readable after its events left the
    // history, until retention cleanup removes the Flow ("retention cleanup
    // deletes owned payload refs from trimmed history", in
    // `audit::retention_values`); the node keeps every replaced version
    // while the Flow lives (audit gap G37).
    let node = open(&directory("audit-history-value-released")?, 4)?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("flow-1")),
            ("type", text("history")),
            ("state", text("s0")),
            ("partition_key", text(P)),
            ("payload", text("payload 0")),
            ("history_max_events", int(2)),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
        ],
    )?;
    let first_ref = field(
        &super::get(&node, "flow-1", &[("partition_key", text(P))])?,
        "payload_ref",
    );
    let mut refs = vec![first_ref];
    for at in 1..=3 {
        ok(
            &node,
            "FLOW.TRANSITION",
            &[
                ("id", text("flow-1")),
                ("partition_key", text(P)),
                ("from_state", text(&format!("s{}", at - 1))),
                ("to_state", text(&format!("s{at}"))),
                ("fencing_token", int(0)),
                ("payload", text(&format!("payload {at}"))),
                ("now_ms", int(1_000 + at)),
            ],
        )?;
        refs.push(field(
            &super::get(&node, "flow-1", &[("partition_key", text(P))])?,
            "payload_ref",
        ));
    }
    // History keeps versions 3 and 4 only; every payload still reads.
    let read = ok(&node, "FLOW.VALUE.MGET", &[("refs", Term::List(refs))])?;
    assert_eq!(
        read,
        Term::List(vec![
            text("payload 0"),
            text("payload 1"),
            text("payload 2"),
            text("payload 3")
        ])
    );
    Ok(())
}
