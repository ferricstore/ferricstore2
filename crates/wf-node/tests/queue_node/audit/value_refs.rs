//! Attached value references (audit gap G9).

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{field, get, int, map, ok, text, texts};
use crate::{TestResult, directory, open};

const P: &str = "tenant-a";

fn node(tag: &str) -> TestResult<QueueNode> {
    open(&directory(tag)?, 4)
}

fn with<'a>(partition: &str, entries: &[(&'a str, Term)]) -> Vec<(&'a str, Term)> {
    let mut all = vec![("partition_key", text(partition))];
    all.extend(entries.iter().cloned());
    all
}

fn create(node: &QueueNode, id: &str, flow_type: &str, extra: &[(&str, Term)]) -> TestResult {
    let mut entries = vec![
        ("id", text(id)),
        ("type", text(flow_type)),
        ("run_at_ms", int(1_000)),
        ("now_ms", int(1_000)),
    ];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

/// Not an Elixir test: audit gap G9 as the 21 Nov probe
/// (`scenario_value_refs.py`) showed Elixir's replies. An attached
/// reference shows without digest or version, reads through `values` when
/// it resolves, replaces an own value of its name and comes back with a
/// rewind.
#[test]
fn attached_references_show_read_replace_and_rewind() -> TestResult {
    let node = node("audit-named-attached")?;
    let shared = ok(
        &node,
        "FLOW.VALUE.PUT",
        &with(P, &[("value", text("shared-bytes"))]),
    )?;
    let shared_ref = field(&shared, "ref");
    create(
        &node,
        "na",
        "named-attached",
        &with(
            P,
            &[
                ("values", map(&[("own", text("own-bytes"))])),
                (
                    "value_refs",
                    map(&[
                        ("shared", shared_ref.clone()),
                        ("external", text("external-ref")),
                    ]),
                ),
            ],
        ),
    )?;
    let fetched = get(
        &node,
        "na",
        &with(P, &[("values", texts(&["own", "shared", "external"]))]),
    )?;
    let refs = field(&fetched, "value_refs");
    assert_eq!(
        field(&refs, "external"),
        map(&[
            ("digest", Term::Nil),
            ("ref", text("external-ref")),
            ("version", Term::Nil),
        ])
    );
    assert_eq!(field(&field(&refs, "shared"), "ref"), shared_ref);
    assert_eq!(
        field(&fetched, "values"),
        map(&[("own", text("own-bytes")), ("shared", text("shared-bytes"))])
    );
    let history = super::events(&node, "na", &with(P, &[]))?;
    let created = ok(&node, "FLOW.HISTORY", &with(P, &[("id", text("na"))]))?;
    let Term::List(entries) = created else {
        return Err("history is not a list".into());
    };
    let Some(Term::List(first)) = entries.first() else {
        return Err("no created event".into());
    };
    let created_id = first.first().cloned().unwrap_or(Term::Nil);
    assert_eq!(history.len(), 1);
    // Attaching "own" replaces the own value; the old version still reads.
    let own_ref = field(&field(&refs, "own"), "ref");
    ok(
        &node,
        "FLOW.CANCEL",
        &with(
            P,
            &[
                ("id", text("na")),
                ("fencing_token", int(0)),
                ("value_refs", map(&[("own", text("elsewhere"))])),
                ("now_ms", int(1_100)),
            ],
        ),
    )?;
    let cancelled = get(&node, "na", &with(P, &[]))?;
    assert_eq!(
        field(&field(&field(&cancelled, "value_refs"), "own"), "ref"),
        text("elsewhere")
    );
    assert_eq!(
        ok(
            &node,
            "FLOW.VALUE.MGET",
            &[("refs", Term::List(vec![own_ref]))]
        )?,
        Term::List(vec![text("own-bytes")])
    );
    ok(
        &node,
        "FLOW.REWIND",
        &with(P, &[("id", text("na")), ("to_event", created_id)]),
    )?;
    let rewound = get(&node, "na", &with(P, &[("values", texts(&["own"]))]))?;
    assert_eq!(
        field(&rewound, "values"),
        map(&[("own", text("own-bytes"))])
    );
    assert_eq!(
        field(&field(&field(&rewound, "value_refs"), "external"), "ref"),
        text("external-ref")
    );
    Ok(())
}
