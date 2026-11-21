//! `flow_test/sections/flow_history_hot_max_rejects_values_above_configured_
//! maximum.exs`, second part: retention cleanup and values other Flows use.
//! A Flow's own values go with it; a value another live Flow references
//! (attached, as a child's input, or in history it keeps) stays until no
//! Flow needs it.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, events, field, get, int, map, ok, refusal, run, text, texts};
use crate::{TestResult, directory, open};

const P: &str = "tenant-shared";

fn in_p<'a>(entries: &[(&'a str, Term)]) -> Vec<(&'a str, Term)> {
    let mut all = vec![("partition_key", text(P))];
    all.extend(entries.iter().cloned());
    all
}

/// A minute past the wall clock, as Elixir's retention tests start.
fn start() -> TestResult<i64> {
    let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
    Ok(i64::try_from(wall.as_millis())? + 60_000)
}

fn create(
    node: &QueueNode,
    id: &str,
    flow_type: &str,
    now: i64,
    extra: &[(&str, Term)],
) -> TestResult {
    let mut entries = in_p(&[
        ("id", text(id)),
        ("type", text(flow_type)),
        ("state", text("queued")),
        ("run_at_ms", int(now)),
        ("now_ms", int(now)),
    ]);
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

/// Claims and completes the one due Flow of `flow_type`; its record.
fn finish(node: &QueueNode, flow_type: &str, now: i64) -> TestResult<Term> {
    let records = claimed(
        node,
        &in_p(&[
            ("type", text(flow_type)),
            ("worker", text("w")),
            ("lease_ms", int(30_000)),
            ("limit", int(1)),
            ("now_ms", int(now)),
        ]),
    )?;
    let [claim] = records.as_slice() else {
        return Err(format!("one claim expected, got {records:?}").into());
    };
    ok(
        node,
        "FLOW.COMPLETE",
        &in_p(&[
            ("id", field(claim, "id")),
            ("lease_token", field(claim, "lease_token")),
            ("fencing_token", field(claim, "fencing_token")),
            ("now_ms", int(now + 1)),
        ]),
    )?;
    let Term::Bin(id) = field(claim, "id") else {
        return Err("claim without id".into());
    };
    get(node, &String::from_utf8(id)?, &in_p(&[]))
}

fn cleanup(node: &QueueNode, now: i64) -> TestResult<i64> {
    let cleaned = ok(
        node,
        "FLOW.RETENTION_CLEANUP",
        &[("limit", int(10)), ("now_ms", int(now))],
    )?;
    match field(&cleaned, "flows") {
        Term::Int(flows) => Ok(flows),
        other => Err(format!("no flow count in {other:?}").into()),
    }
}

fn put(node: &QueueNode, value: &str, owner: Option<&str>, now: i64) -> TestResult<Term> {
    let mut entries = in_p(&[("value", text(value)), ("now_ms", int(now))]);
    if let Some(owner) = owner {
        entries.extend([("owner_flow_id", text(owner)), ("name", text("doc"))]);
    }
    Ok(field(&ok(node, "FLOW.VALUE.PUT", &entries)?, "ref"))
}

fn mget(node: &QueueNode, refs: Vec<Term>) -> TestResult<Term> {
    ok(node, "FLOW.VALUE.MGET", &[("refs", Term::List(refs))])
}

fn values_of(node: &QueueNode, id: &str) -> TestResult<Term> {
    Ok(field(
        &get(node, id, &in_p(&[("values", texts(&["doc"]))]))?,
        "values",
    ))
}

fn doc(reference: &Term) -> Term {
    map(&[("doc", reference.clone())])
}

#[test]
fn a_shared_value_stays_while_another_flow_attaches_it() -> TestResult {
    // Elixir: "retention cleanup preserves shared value refs used by
    // another flow".
    let node = open(&directory("audit-shared-two")?, 4)?;
    let now = start()?;
    let shared = put(&node, "shared-doc", None, now)?;
    for id in ["a", "b"] {
        let extra = [("value_refs", doc(&shared)), ("retention_ttl_ms", int(20))];
        create(&node, id, "shared-ref", now, &extra)?;
    }
    let completed = finish(&node, "shared-ref", now + 1_000)?;
    let Term::Int(until) = field(&completed, "terminal_retention_until_ms") else {
        return Err("no retention deadline".into());
    };
    assert_eq!(until, now + 1_021);
    assert_eq!(cleanup(&node, now + 2_000)?, 1);
    assert_eq!(
        mget(&node, vec![shared])?,
        Term::List(vec![text("shared-doc")])
    );
    let survivor = if field(&completed, "id") == text("a") {
        "b"
    } else {
        "a"
    };
    assert_eq!(
        values_of(&node, survivor)?,
        map(&[("doc", text("shared-doc"))])
    );
    Ok(())
}

#[test]
fn a_flow_s_own_payload_and_unreferenced_named_values_go_with_it() -> TestResult {
    // Elixir: "retention cleanup does not global-scan refs for private
    // payload values" and "retention cleanup reclaims unreferenced owner
    // named shared values without scanning" (the scans are Elixir's own).
    let node = open(&directory("audit-shared-own")?, 4)?;
    let now = start()?;
    let extra = [
        ("payload", text("private-payload")),
        ("retention_ttl_ms", int(20)),
    ];
    create(&node, "private", "private-payload", now, &extra)?;
    let payload = field(&get(&node, "private", &in_p(&[]))?, "payload_ref");
    finish(&node, "private-payload", now + 1_000)?;
    create(
        &node,
        "owner",
        "owner-only",
        now,
        &[("retention_ttl_ms", int(20))],
    )?;
    let named = put(&node, "shared-doc", Some("owner"), now + 1)?;
    finish(&node, "owner-only", now + 1_000)?;
    assert_eq!(cleanup(&node, now + 2_000)?, 2);
    assert_eq!(
        mget(&node, vec![payload, named])?,
        Term::List(vec![Term::Nil, Term::Nil])
    );
    Ok(())
}

#[test]
fn an_owner_s_named_value_stays_while_a_live_flow_attaches_it() -> TestResult {
    // Elixir: "retention cleanup preserves owner named values referenced by
    // a live flow" and "bounded partial and full cleanup preserve a shared
    // value with a live consumer".
    let node = open(&directory("audit-shared-owner")?, 4)?;
    let now = start()?;
    create(
        &node,
        "owner",
        "owned-owner",
        now,
        &[("retention_ttl_ms", int(20))],
    )?;
    let named = put(&node, "shared-doc", Some("owner"), now + 1)?;
    assert_eq!(
        values_of(&node, "owner")?,
        map(&[("doc", text("shared-doc"))])
    );
    let extra = [("value_refs", doc(&named)), ("retention_ttl_ms", int(20))];
    create(&node, "consumer", "owned-consumer", now + 2, &extra)?;
    finish(&node, "owned-owner", now + 1_000)?;
    assert_eq!(cleanup(&node, now + 2_000)?, 1);
    assert_eq!(get(&node, "owner", &in_p(&[]))?, Term::Nil);
    assert_eq!(cleanup(&node, now + 2_001)?, 0);
    assert_eq!(
        mget(&node, vec![named.clone()])?,
        Term::List(vec![text("shared-doc")])
    );
    assert_eq!(
        values_of(&node, "consumer")?,
        map(&[("doc", text("shared-doc"))])
    );
    let consumer = finish(&node, "owned-consumer", now + 2_002)?;
    assert_eq!(field(&consumer, "state"), text("completed"));
    assert_eq!(cleanup(&node, now + 2_200)?, 1);
    assert_eq!(mget(&node, vec![named])?, Term::List(vec![Term::Nil]));
    Ok(())
}

#[test]
fn a_flow_on_another_shard_cannot_attach_a_value() -> TestResult {
    // Elixir: "retention cleanup rejects a racing cross-shard shared value
    // consumer" (the race is Elixir's; the refusal is what a client sees).
    let node = open(&directory("audit-shared-slot")?, 4)?;
    let now = start()?;
    create(&node, "owner", "racing-owner", now, &[])?;
    let named = put(&node, "shared-doc", Some("owner"), now + 1)?;
    let elsewhere = (0..64)
        .map(|index| format!("other-{index}"))
        .find(|partition| partition.as_str() != P)
        .ok_or("no partition")?;
    let consumer = run(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("consumer")),
            ("type", text("racing-consumer")),
            ("partition_key", text(&elsewhere)),
            ("value_refs", doc(&named)),
            ("now_ms", int(now + 2)),
        ],
    )?;
    let refused = refusal(consumer)?;
    assert!(refused.starts_with("CROSSSLOT"), "{refused}");
    Ok(())
}

#[test]
fn a_child_s_attached_value_outlives_its_owner() -> TestResult {
    // Elixir: "same-shard child shared ref acquisition ordered before
    // cleanup preserves the value".
    let node = open(&directory("audit-shared-child")?, 4)?;
    let now = start()?;
    create(
        &node,
        "owner",
        "spawn-owner",
        now,
        &[("retention_ttl_ms", int(20))],
    )?;
    let named = put(&node, "shared-spawn", Some("owner"), now + 1)?;
    create(&node, "parent", "spawn-parent", now, &[])?;
    let records = claimed(
        &node,
        &in_p(&[
            ("type", text("spawn-parent")),
            ("worker", text("w")),
            ("limit", int(1)),
            ("now_ms", int(now + 2)),
        ]),
    )?;
    let [parent] = records.as_slice() else {
        return Err("one parent claim".into());
    };
    finish(&node, "spawn-owner", now + 1_000)?;
    let child = map(&[
        ("id", text("child")),
        ("type", text("spawn-child")),
        ("partition_key", text(P)),
        ("value_refs", doc(&named)),
    ]);
    ok(
        &node,
        "FLOW.SPAWN_CHILDREN",
        &in_p(&[
            ("id", text("parent")),
            ("lease_token", field(parent, "lease_token")),
            ("fencing_token", field(parent, "fencing_token")),
            ("children", Term::List(vec![child])),
            ("group_id", text("spawn-group")),
            ("wait", text("all")),
            ("wait_state", text("waiting_children")),
            ("success", text("completed")),
            ("failure", text("failed")),
            ("now_ms", int(now + 1_002)),
        ]),
    )?;
    assert_eq!(cleanup(&node, now + 2_000)?, 1);
    assert_eq!(get(&node, "owner", &in_p(&[]))?, Term::Nil);
    assert_eq!(
        mget(&node, vec![named])?,
        Term::List(vec![text("shared-spawn")])
    );
    assert_eq!(
        values_of(&node, "child")?,
        map(&[("doc", text("shared-spawn"))])
    );
    Ok(())
}

#[test]
fn a_value_named_in_kept_history_outlives_its_owner() -> TestResult {
    // Elixir: "retention cleanup preserves owner named values referenced by
    // retained history".
    let node = open(&directory("audit-shared-history")?, 4)?;
    let now = start()?;
    create(
        &node,
        "owner",
        "history-owner",
        now,
        &[("retention_ttl_ms", int(20))],
    )?;
    let named = put(&node, "shared-doc", Some("owner"), now + 1)?;
    create(
        &node,
        "consumer",
        "history-consumer",
        now + 2,
        &[("value_refs", doc(&named))],
    )?;
    ok(
        &node,
        "FLOW.TRANSITION",
        &in_p(&[
            ("id", text("consumer")),
            ("from_state", text("queued")),
            ("to_state", text("ready")),
            ("fencing_token", int(0)),
            ("drop_values", texts(&["doc"])),
            ("now_ms", int(now + 3)),
        ]),
    )?;
    let consumer = get(&node, "consumer", &in_p(&[]))?;
    assert_eq!(field(&field(&consumer, "value_refs"), "doc"), Term::Nil);
    finish(&node, "history-owner", now + 1_000)?;
    let created_refs = |history: Vec<Term>| -> Term {
        history
            .iter()
            .find(|fields| field(fields, "event") == text("created"))
            .map_or(Term::Nil, |fields| field(fields, "value_refs"))
    };
    let options = in_p(&[("count", int(10)), ("values", Term::Bool(true))]);
    let before = created_refs(events(&node, "consumer", &options)?);
    assert_ne!(before, Term::Nil);
    assert_eq!(cleanup(&node, now + 2_000)?, 1);
    assert_eq!(
        mget(&node, vec![named])?,
        Term::List(vec![text("shared-doc")])
    );
    assert_eq!(created_refs(events(&node, "consumer", &options)?), before);
    Ok(())
}

#[test]
fn cleanup_of_a_flow_keeps_values_of_ids_it_prefixes() -> TestResult {
    // Elixir: "retention cleanup does not delete generated values for
    // colon-prefixed flow ids".
    let node = open(&directory("audit-shared-prefix")?, 4)?;
    let now = start()?;
    let own = |payload: &str| [("payload", text(payload)), ("retention_ttl_ms", int(10))];
    create(&node, "x", "prefix-parent", now, &own("parent-payload"))?;
    create(&node, "x:child", "prefix-child", now, &own("child-payload"))?;
    let parent_ref = field(&get(&node, "x", &in_p(&[]))?, "payload_ref");
    let child_ref = field(&get(&node, "x:child", &in_p(&[]))?, "payload_ref");
    assert_ne!(parent_ref, child_ref);
    let completed = finish(&node, "prefix-parent", now)?;
    assert_eq!(
        field(&completed, "terminal_retention_until_ms"),
        int(now + 11)
    );
    assert_eq!(cleanup(&node, now + 1_000)?, 1);
    assert_eq!(
        mget(&node, vec![parent_ref, child_ref.clone()])?,
        Term::List(vec![Term::Nil, text("child-payload")])
    );
    let child = get(&node, "x:child", &in_p(&[("values", Term::Bool(true))]))?;
    assert_eq!(field(&child, "payload_ref"), child_ref);
    Ok(())
}

#[test]
fn held_values_survive_snapshots_and_restarts() -> TestResult {
    // Audit gap G10 across restarts: a finished holder on disk still holds
    // after a snapshot, and a value moved aside stays until its last holder
    // goes, under any shard count.
    let path = directory("audit-shared-restart")?;
    let now = start()?;
    let named = {
        let node = open(&path, 4)?;
        create(
            &node,
            "owner",
            "restart-owner",
            now,
            &[("retention_ttl_ms", int(20))],
        )?;
        let named = put(&node, "kept-doc", Some("owner"), now + 1)?;
        let extra = [
            ("value_refs", doc(&named)),
            ("retention_ttl_ms", int(100_000)),
        ];
        create(&node, "consumer", "restart-consumer", now + 2, &extra)?;
        finish(&node, "restart-owner", now + 1_000)?;
        finish(&node, "restart-consumer", now + 1_000)?;
        node.snapshot()?;
        named
    };
    {
        let node = open(&path, 3)?;
        assert_eq!(cleanup(&node, now + 2_000)?, 1);
        assert_eq!(get(&node, "owner", &in_p(&[]))?, Term::Nil);
        assert_eq!(
            mget(&node, vec![named.clone()])?,
            Term::List(vec![text("kept-doc")])
        );
        node.snapshot()?;
    }
    let node = open(&path, 4)?;
    assert_eq!(
        mget(&node, vec![named.clone()])?,
        Term::List(vec![text("kept-doc")])
    );
    assert_eq!(cleanup(&node, now + 200_000)?, 1);
    assert_eq!(mget(&node, vec![named])?, Term::List(vec![Term::Nil]));
    Ok(())
}
