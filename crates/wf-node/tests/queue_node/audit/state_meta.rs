//! `flow_state_meta_test.exs`: per-state metadata is kept per state across
//! later states and restarts, claims keep type and state names holding
//! separators, searches find it by the SDK's `FLOW.QUERY` text (before and
//! after the index policy, and in terminal states, until retention removes
//! the Flow), the indexed key follows the policy, and entries stay bounded.
//! (Its LMDB rows, migration worker and value discovery API are Elixir's.)

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, field, get, int, map, ok, run, text, texts};
use crate::{TestResult, directory, open};

const P: &str = "tenant-state-meta";

fn in_p() -> [(&'static str, Term); 1] {
    [("partition_key", text(P))]
}

/// Creates `id` of `flow_type` in `state` with `meta` as its state
/// metadata; `extra` adds options.
fn create(
    node: &QueueNode,
    id: &str,
    (flow_type, state): (&str, &str),
    meta: Term,
    extra: &[(&str, Term)],
) -> TestResult<Result<Term, String>> {
    let mut entries = vec![
        ("id", text(id)),
        ("type", text(flow_type)),
        ("state", text(state)),
        ("partition_key", text(P)),
        ("state_meta", meta),
        ("run_at_ms", int(1_000)),
        ("now_ms", int(1_000)),
    ];
    entries.extend(extra.iter().cloned());
    Ok(run(node, "FLOW.CREATE", &entries)?.map_err(|error| error.to_string()))
}

fn claim(node: &QueueNode, flow_type: &str, state: &str) -> TestResult<Term> {
    let records = claimed(
        node,
        &[
            ("type", text(flow_type)),
            ("states", texts(&[state])),
            ("partition_key", text(P)),
            ("worker", text("worker-a")),
            ("limit", int(1)),
            ("now_ms", int(1_001)),
        ],
    )?;
    match records.as_slice() {
        [record] => Ok(record.clone()),
        _ => Err(format!("one claim expected, got {records:?}").into()),
    }
}

/// Moves the claimed Flow from `running` to `to` with `meta`.
fn transition(
    node: &QueueNode,
    lease: &Term,
    to: &str,
    meta: Term,
) -> TestResult<Result<Term, String>> {
    let entries = [
        ("id", field(lease, "id")),
        ("from_state", text("running")),
        ("to_state", text(to)),
        ("lease_token", field(lease, "lease_token")),
        ("fencing_token", field(lease, "fencing_token")),
        ("partition_key", text(P)),
        ("state_meta", meta),
        ("run_at_ms", int(1_010)),
        ("now_ms", int(1_002)),
    ];
    Ok(run(node, "FLOW.TRANSITION", &entries)?.map_err(|error| error.to_string()))
}

fn complete(node: &QueueNode, lease: &Term, meta: Term, extra: &[(&str, Term)]) -> TestResult {
    let mut entries = vec![
        ("id", field(lease, "id")),
        ("lease_token", field(lease, "lease_token")),
        ("fencing_token", field(lease, "fencing_token")),
        ("partition_key", text(P)),
        ("state_meta", meta),
        ("now_ms", int(1_002)),
    ];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.COMPLETE", &entries)?;
    Ok(())
}

fn index(node: &QueueNode, flow_type: &str, key: Term) -> TestResult<Result<Term, String>> {
    let set = run(
        node,
        "FLOW.POLICY.SET",
        &[("type", text(flow_type)), ("indexed_state_meta", key)],
    )?;
    Ok(set.map_err(|error| error.to_string()))
}

/// The ids the SDK's search text finds for `state`'s metadata `name` =
/// `value` among `flow_type`'s Flows in `P`.
fn search(
    node: &QueueNode,
    flow_type: &str,
    (state, name): (&str, &str),
    value: Term,
) -> TestResult<Vec<Term>> {
    let query = format!(
        "FROM runs WHERE partition_key = @partition_key AND type = @type AND state_meta['{state}']['{name}'] = @state_meta_0 ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS"
    );
    let result = ok(
        node,
        "FLOW.QUERY",
        &[
            ("version", text("FQL1")),
            ("query", text(&query)),
            (
                "params",
                map(&[
                    ("partition_key", text(P)),
                    ("type", text(flow_type)),
                    ("state_meta_0", value),
                ]),
            ),
        ],
    )?;
    let Term::List(records) = field(&result, "records") else {
        return Err(format!("no records in {result:?}").into());
    };
    Ok(records.iter().map(|record| field(record, "id")).collect())
}

fn numbered(count: i64) -> Term {
    let entries: Vec<(String, Term)> = (1..=count)
        .map(|index| (format!("k{index}"), int(index)))
        .collect();
    let borrowed: Vec<(&str, Term)> = entries
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect();
    map(&borrowed)
}

#[test]
fn state_metadata_is_kept_per_state_across_states_and_restarts() -> TestResult {
    // Elixir: "state_meta stores durable per-state metadata and survives
    // later states" (its codec round trip as a restart).
    let path = directory("audit-state-meta-kept")?;
    let accept = map(&[("version", int(1)), ("owner", text("risk"))]);
    let expected = {
        let node = open(&path, 4)?;
        create(&node, "kept", ("state-meta", "accept"), accept.clone(), &[])??;
        let record = get(&node, "kept", &in_p())?;
        assert_eq!(
            super::sorted(&field(&record, "state_meta")),
            super::sorted(&map(&[("accept", accept.clone())]))
        );
        let lease = claim(&node, "state-meta", "accept")?;
        transition(&node, &lease, "wow", map(&[("version", int(2))]))??;
        let expected = map(&[("accept", accept), ("wow", map(&[("version", int(2))]))]);
        assert_eq!(
            super::sorted(&field(&get(&node, "kept", &in_p())?, "state_meta")),
            super::sorted(&expected)
        );
        expected
    };
    let node = open(&path, 4)?;
    assert_eq!(
        super::sorted(&field(&get(&node, "kept", &in_p())?, "state_meta")),
        super::sorted(&expected)
    );
    Ok(())
}

#[test]
fn a_claim_keeps_type_and_state_names_holding_separators() -> TestResult {
    // Elixir: "claim preserves separator-bearing type and state index
    // components".
    let node = open(&directory("audit-state-meta-separators")?, 4)?;
    let (flow_type, state) = ("state:meta:type", "accept:manual");
    create(
        &node,
        "separated",
        (flow_type, state),
        map(&[("version", int(1))]),
        &[],
    )??;
    let lease = claim(&node, flow_type, state)?;
    assert_eq!(field(&lease, "id"), text("separated"));
    assert_eq!(field(&lease, "type"), text(flow_type));
    assert_eq!(field(&lease, "state"), text("running"));
    assert_eq!(field(&lease, "run_state"), text(state));
    assert_eq!(
        field(&lease, "state_meta"),
        map(&[(state, map(&[("version", int(1))]))])
    );
    Ok(())
}

#[test]
fn searches_find_state_metadata_before_and_after_the_index_policy() -> TestResult {
    // Elixir: "state_meta version is queryable only when policy indexes
    // that metadata key", "indexed state_meta policy backfills existing
    // flow records" and "indexed state_meta can query terminal state
    // metadata".
    let node = open(&directory("audit-state-meta-search")?, 4)?;
    let version = |value| map(&[("version", int(value))]);
    index(&node, "indexed", text("version"))??;
    create(&node, "indexed-a", ("indexed", "accept"), version(1), &[])??;
    create(&node, "indexed-b", ("indexed", "accept"), version(9), &[])??;
    let lease = claim(&node, "indexed", "accept")?;
    transition(&node, &lease, "wow", version(2))??;
    let found = search(&node, "indexed", ("accept", "version"), int(1))?;
    assert_eq!(found, [field(&lease, "id")]);

    create(&node, "late", ("backfill", "accept"), version(1), &[])??;
    let lease = claim(&node, "backfill", "accept")?;
    complete(&node, &lease, version(3), &[])?;
    index(&node, "backfill", text("version"))??;
    assert_eq!(
        search(&node, "backfill", ("accept", "version"), int(1))?,
        [text("late")]
    );
    assert_eq!(
        search(&node, "backfill", ("completed", "version"), int(3))?,
        [text("late")]
    );
    Ok(())
}

#[test]
fn the_indexed_key_follows_the_policy() -> TestResult {
    // Elixir: "changing or removing indexed state_meta policy deletes stale
    // LMDB query rows" (its rows as the record's indexed key and searches).
    let node = open(&directory("audit-state-meta-policy")?, 4)?;
    index(&node, "changing", text("version"))??;
    let meta = map(&[("version", int(1)), ("owner", text("risk"))]);
    create(&node, "changed", ("changing", "accept"), meta, &[])??;
    let indexed =
        || get(&node, "changed", &in_p()).map(|record| field(&record, "indexed_state_meta"));
    assert_eq!(indexed()?, text("version"));
    index(&node, "changing", text("owner"))??;
    assert_eq!(indexed()?, text("owner"));
    assert_eq!(
        search(&node, "changing", ("accept", "owner"), text("risk"))?,
        [text("changed")]
    );
    index(&node, "changing", Term::Nil)??;
    assert_eq!(indexed()?, Term::Nil);
    Ok(())
}

#[test]
fn retention_cleanup_removes_a_finished_flow_from_searches() -> TestResult {
    // Elixir: "retention cleanup deletes indexed state_meta query rows from
    // LMDB".
    let node = open(&directory("audit-state-meta-retention")?, 4)?;
    index(&node, "retained", text("version"))??;
    let ttl = [("retention_ttl_ms", int(60_000))];
    create(
        &node,
        "retained",
        ("retained", "accept"),
        map(&[("version", int(1))]),
        &ttl,
    )??;
    let lease = claim(&node, "retained", "accept")?;
    complete(&node, &lease, map(&[("version", int(3))]), &[])?;
    assert_eq!(
        search(&node, "retained", ("completed", "version"), int(3))?,
        [text("retained")]
    );
    let Term::Int(until) = field(
        &get(&node, "retained", &in_p())?,
        "terminal_retention_until_ms",
    ) else {
        return Err("no retention deadline".into());
    };
    let cleaned = ok(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[("limit", int(10)), ("now_ms", int(until.saturating_add(1)))],
    )?;
    assert!(matches!(field(&cleaned, "flows"), Term::Int(flows) if flows >= 1));
    assert_eq!(get(&node, "retained", &in_p())?, Term::Nil);
    assert_eq!(
        search(&node, "retained", ("completed", "version"), int(3))?,
        Vec::<Term>::new()
    );
    Ok(())
}

#[test]
fn state_metadata_and_its_index_stay_bounded() -> TestResult {
    // Elixir: "state_meta and indexed_state_meta stay bounded", "rejects a
    // state_meta merge that would overflow an existing state entry" and
    // "state_meta update rejects growth beyond the entry cap atomically".
    let node = open(&directory("audit-state-meta-bounds")?, 4)?;
    let too_many = create(&node, "too-many", ("bounded", "accept"), numbered(17), &[])?;
    assert_eq!(
        too_many,
        Err("ERR too many flow state_meta entries".to_owned())
    );
    let two_keys = index(&node, "bounded", texts(&["version", "owner"]))?;
    assert_eq!(
        two_keys,
        Err("ERR flow indexed_state_meta supports at most 1 key".to_owned())
    );
    for (id, state, added) in [
        ("overflow", "queued", "overflow"),
        ("growth", "accept", "k17"),
    ] {
        let flow_type = format!("bounded-{id}");
        create(&node, id, (&flow_type, state), numbered(16), &[])??;
        let lease = claim(&node, &flow_type, state)?;
        let refused = transition(&node, &lease, state, map(&[(added, int(17))]))?;
        assert_eq!(
            refused,
            Err("ERR too many flow state_meta entries".to_owned()),
            "{id}"
        );
        let record = get(&node, id, &in_p())?;
        assert_eq!(field(&record, "state"), text("running"));
        assert_eq!(field(&record, "version"), field(&lease, "version"));
        assert_eq!(
            super::sorted(&field(&field(&record, "state_meta"), state)),
            super::sorted(&numbered(16))
        );
    }
    Ok(())
}

#[test]
fn null_values_and_64_bit_bounds_are_kept() -> TestResult {
    // Elixir: "explicit null state_meta values survive the durable codec"
    // and "rejects state_meta numbers that cannot be represented by query
    // and native codecs" (its out-of-range integers cannot be sent: the
    // protocol's integers are 64-bit).
    let node = open(&directory("audit-state-meta-values")?, 4)?;
    create(
        &node,
        "nullable",
        ("nulls", "queued"),
        map(&[("nullable", Term::Nil)]),
        &[],
    )??;
    let queued = field(
        &field(&get(&node, "nullable", &in_p())?, "state_meta"),
        "queued",
    );
    let Term::Map(entries) = queued else {
        return Err(format!("state metadata expected, got {queued:?}").into());
    };
    assert_eq!(entries, vec![(b"nullable".to_vec(), Term::Nil)]);
    let bounds = map(&[("minimum", int(i64::MIN)), ("maximum", int(i64::MAX))]);
    create(&node, "bounds", ("bounds", "accept"), bounds.clone(), &[])??;
    let kept = field(
        &field(&get(&node, "bounds", &in_p())?, "state_meta"),
        "accept",
    );
    assert_eq!(super::sorted(&kept), super::sorted(&bounds));
    Ok(())
}
