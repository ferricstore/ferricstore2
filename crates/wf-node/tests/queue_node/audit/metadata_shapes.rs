//! `flow/metadata_validation_test.exs`, `flow/retry_policy_test.exs` and
//! `flow_facade_correctness_test.exs`, as the wire shows them: attribute
//! lists are non-empty and kept unique, names are trimmed and valid UTF-8;
//! a state policy is a map, absent policy fields stay absent, and each
//! state keeps its own mode; a read's hydration stays within its ceiling.
//! (Their persisted-shape predicates, corrupt-record fixtures and source
//! checks are Elixir's.)

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{field, get, int, map, ok, run, text, texts};
use crate::{TestResult, directory, open};

const P: &str = "tenant-metadata";

fn create(node: &QueueNode, id: &str, extra: &[(&str, Term)]) -> TestResult<Result<Term, String>> {
    let mut entries = vec![
        ("id", text(id)),
        ("type", text("metadata")),
        ("partition_key", text(P)),
    ];
    entries.extend(extra.iter().cloned());
    Ok(run(node, "FLOW.CREATE", &entries)?.map_err(|error| error.to_string()))
}

#[test]
fn attribute_lists_and_names_are_normalized_or_refused() -> TestResult {
    // Elixir: "attributes identify canonical persisted metadata without
    // rebuilding it", "state metadata identifies canonical scalar-only
    // persisted metadata" and "normalization rejects names that cannot be
    // addressed by FQL".
    let node = open(&directory("audit-metadata-names")?, 4)?;
    let empty = create(
        &node,
        "empty",
        &[("attributes", map(&[("list", Term::List(Vec::new()))]))],
    )?;
    assert_eq!(
        empty,
        Err("ERR flow attribute list must not be empty".to_owned())
    );
    create(
        &node,
        "kept",
        &[(
            "attributes",
            map(&[
                (" padded", text("value")),
                ("list", texts(&["same", "same", "other"])),
            ]),
        )],
    )??;
    let attributes = field(
        &get(&node, "kept", &[("partition_key", text(P))])?,
        "attributes",
    );
    assert_eq!(field(&attributes, "padded"), text("value"));
    assert_eq!(field(&attributes, " padded"), Term::Nil);
    assert_eq!(field(&attributes, "list"), texts(&["same", "other"]));
    let bad_key = Term::Map(vec![(vec![0xFF], text("value"))]);
    let refused = create(&node, "bad-key", &[("attributes", bad_key)])?;
    assert_eq!(
        refused,
        Err("ERR flow attribute key must be valid UTF-8".to_owned())
    );
    // A create's state metadata is the flat map of its own state, so a
    // name that is not UTF-8 is a key there.
    let bad_meta = Term::Map(vec![(vec![0xFF], text("one"))]);
    let refused = create(&node, "bad-meta", &[("state_meta", bad_meta)])?;
    assert_eq!(
        refused,
        Err("ERR flow state_meta key must be valid UTF-8".to_owned())
    );
    Ok(())
}

fn policy_set(node: &QueueNode, flow_type: &str, states: Term) -> TestResult<Result<Term, String>> {
    let set = run(
        node,
        "FLOW.POLICY.SET",
        &[("type", text(flow_type)), ("states", states)],
    )?;
    Ok(set.map_err(|error| error.to_string()))
}

#[test]
fn a_state_policy_is_a_map_and_keeps_its_own_mode() -> TestResult {
    // Elixir: "flow policies reject malformed per-state policy values",
    // "flow policies omit absent fields from their canonical snapshot" and
    // "each state retains its independently configured execution mode".
    let node = open(&directory("audit-metadata-policy")?, 4)?;
    let malformed = policy_set(&node, "invalid-state-policy", map(&[("queued", int(42))]))?;
    assert_eq!(
        malformed,
        Err("ERR flow state policy must be a map or keyword list".to_owned())
    );
    let compact = policy_set(&node, "compact-policy", map(&[("queued", map(&[]))]))??;
    assert_eq!(field(&compact, "version"), Term::Nil);
    assert_eq!(field(&compact, "max_active_ms"), Term::Nil);
    let modes = map(&[
        ("queued", map(&[("mode", text("fifo"))])),
        ("review", map(&[("mode", text("parallel"))])),
    ]);
    policy_set(&node, "ordered-policy", modes)??;
    let policy = ok(
        &node,
        "FLOW.POLICY.GET",
        &[("type", text("ordered-policy"))],
    )?;
    let mode = |state: &str| field(&field(&field(&policy, "states"), state), "mode");
    assert_eq!(mode("queued"), text("fifo"));
    assert_eq!(mode("review"), text("parallel"));
    Ok(())
}

#[test]
fn a_read_keeps_hydration_within_its_ceiling() -> TestResult {
    // Elixir: "get rejects payload hydration above the configured ceiling".
    let node = open(&directory("audit-metadata-get-ceiling")?, 4)?;
    let read = run(
        &node,
        "FLOW.GET",
        &[
            ("id", text("payload-ceiling")),
            ("payload_max_bytes", int(65_537)),
        ],
    )?;
    assert_eq!(
        read.map_err(|error| error.to_string()),
        Err("ERR flow payload_max_bytes exceeds maximum 65536".to_owned())
    );
    Ok(())
}
