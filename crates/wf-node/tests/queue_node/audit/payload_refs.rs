//! A stored value as a Flow's payload (audit gap G16):
//! `flow_internal_keys_use_compact_partition_tags.exs` (its tests that
//! `flow_create` and `flow_transition` accept `payload_ref`, and that managed
//! shared value refs must share the Flow hash slot) and the payload cases of
//! `flow_history_hot_max_rejects_values_above_configured_maximum.exs`
//! ("retention cleanup preserves an owner shared value used as a payload
//! ref", "retention tracks named payload result and error shared refs").

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, field, get, int, ok, refusal, run, text};
use crate::{TestResult, directory, open};

fn create(node: &QueueNode, id: &str, extra: &[(&str, Term)]) -> TestResult<Result<Term, String>> {
    let mut entries = vec![
        ("id", text(id)),
        ("type", text("checkout")),
        ("state", text("queued")),
        ("run_at_ms", int(1_000)),
        ("now_ms", int(1_000)),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    Ok(run(node, "FLOW.CREATE", &entries)?.map_err(|error| error.to_string()))
}

fn put(node: &QueueNode, partition: &str, value: &str, owner: Option<&str>) -> TestResult<Term> {
    let mut entries = vec![
        ("value", text(value)),
        ("partition_key", text(partition)),
        ("now_ms", int(1_000)),
    ];
    if let Some(owner) = owner {
        entries.extend([("owner_flow_id", text(owner)), ("name", text("payload"))]);
    }
    Ok(field(&ok(node, "FLOW.VALUE.PUT", &entries)?, "ref"))
}

#[test]
fn create_and_transition_take_a_payload_reference() -> TestResult {
    let path = directory("audit-pref-opaque")?;
    let node = open(&path, 4)?;
    let initial = text("shared:payload:initial");
    create(&node, "o", &[("payload_ref", initial.clone())])??;
    assert_eq!(field(&get(&node, "o", &[])?, "payload_ref"), initial);
    ok(
        &node,
        "FLOW.TRANSITION",
        &[
            ("id", text("o")),
            ("from_state", text("queued")),
            ("to_state", text("ready")),
            ("fencing_token", int(0)),
            ("payload_ref", text("shared:payload:transition")),
            ("run_at_ms", int(2_000)),
            ("now_ms", int(1_100)),
        ],
    )?;
    let moved = get(&node, "o", &[])?;
    assert_eq!(field(&moved, "state"), text("ready"));
    assert_eq!(
        field(&moved, "payload_ref"),
        text("shared:payload:transition")
    );
    // The reference survives a reopen under another shard count.
    drop(node);
    let node = open(&path, 3)?;
    assert_eq!(
        field(&get(&node, "o", &[])?, "payload_ref"),
        text("shared:payload:transition")
    );
    Ok(())
}

#[test]
fn a_shared_payload_reference_must_share_the_flow_s_slot_and_reads_as_the_payload() -> TestResult {
    let node = open(&directory("audit-pref-slot")?, 4)?;
    let shared = put(&node, "here", "shared-body", None)?;
    let elsewhere = create(
        &node,
        "far",
        &[
            ("partition_key", text("there")),
            ("payload_ref", shared.clone()),
        ],
    )?;
    assert_eq!(
        elsewhere,
        Err("CROSSSLOT Flow shared value refs must hash to the Flow shard".to_owned())
    );
    create(
        &node,
        "near",
        &[
            ("partition_key", text("here")),
            ("payload_ref", shared.clone()),
        ],
    )??;
    let in_p = [("partition_key", text("here"))];
    let local = get(&node, "near", &in_p)?;
    assert_eq!(field(&local, "payload_ref"), shared);
    let full = get(
        &node,
        "near",
        &[("partition_key", text("here")), ("full", Term::Bool(true))],
    )?;
    assert_eq!(field(&full, "payload"), text("shared-body"));
    let records = claimed(
        &node,
        &[
            ("type", text("checkout")),
            ("partition_key", text("here")),
            ("worker", text("w")),
            ("limit", int(1)),
            ("now_ms", int(1_000)),
        ],
    )?;
    let [record] = records.as_slice() else {
        return Err("one claim".into());
    };
    assert_eq!(field(record, "payload"), text("shared-body"));
    Ok(())
}

#[test]
fn an_owner_s_value_used_as_a_payload_outlives_the_owner() -> TestResult {
    let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
    let future = i64::try_from(wall.as_millis())? + 60_000;
    let node = open(&directory("audit-pref-owner")?, 4)?;
    let in_p = [("partition_key", text("p"))];
    let owner = [("partition_key", text("p")), ("retention_ttl_ms", int(20))];
    create(&node, "owner", &owner)?
        .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
    let named = put(&node, "p", "shared-payload", Some("owner"))?;
    let consumer = [
        ("partition_key", text("p")),
        ("payload_ref", named.clone()),
        ("type", text("consumer")),
    ];
    create(&node, "consumer", &consumer)??;
    let records = claimed(
        &node,
        &[
            ("type", text("checkout")),
            ("partition_key", text("p")),
            ("worker", text("w")),
            ("limit", int(1)),
            ("now_ms", int(1_000)),
        ],
    )?;
    let [lease] = records.as_slice() else {
        return Err("one claim".into());
    };
    assert_eq!(field(lease, "id"), text("owner"));
    ok(
        &node,
        "FLOW.COMPLETE",
        &[
            ("id", text("owner")),
            ("partition_key", text("p")),
            ("lease_token", field(lease, "lease_token")),
            ("fencing_token", field(lease, "fencing_token")),
            // A minute ahead, so the shard's own sweep leaves it to the
            // cleanup below.
            ("now_ms", int(future)),
        ],
    )?;
    let later = future + 2_000;
    let cleaned = ok(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[("limit", int(10)), ("now_ms", int(later))],
    )?;
    assert_eq!(field(&cleaned, "flows"), int(1));
    assert_eq!(get(&node, "owner", &in_p)?, Term::Nil);
    let read = ok(
        &node,
        "FLOW.VALUE.MGET",
        &[("refs", Term::List(vec![named]))],
    )?;
    assert_eq!(read, Term::List(vec![text("shared-payload")]));
    let full = get(
        &node,
        "consumer",
        &[("partition_key", text("p")), ("full", Term::Bool(true))],
    )?;
    assert_eq!(field(&full, "payload"), text("shared-payload"));
    let refused = run(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("x")),
            ("type", text("t")),
            ("payload_ref", int(5)),
        ],
    )?;
    assert!(refusal(refused)?.starts_with("ERR flow payload_ref"));
    Ok(())
}
