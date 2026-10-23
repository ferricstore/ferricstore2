//! Pinned `ManyItemOpts` source cases: async: true, :flow, no local setup.
//! Canonical proper-list/map capture and opaque-value correspondence are caller inputs.

#![forbid(unsafe_code)]

use wf_domain::policy::complete::RawFlowPolicyValue as Raw;
use wf_domain::workflow::caller::many::{self, ManyItemOptionsError, ParsedManyItem};

mod edges;
mod maps;

fn rows_match(actual: &[(&str, &Raw<'_>)], expected: &[(&str, &Raw<'_>)]) -> bool {
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected)
            .all(|((key, value), (wanted_key, wanted_value))| {
                key == wanted_key && std::ptr::eq(*value, *wanted_value)
            })
}

fn parsed_match(
    actual: &Result<ParsedManyItem<'_>, ManyItemOptionsError>,
    id: &[u8],
    lease: Option<&[u8]>,
    expected_rows: &[(&str, &Raw<'_>)],
) -> bool {
    actual.as_ref().is_ok_and(|parsed| {
        parsed.id == id && parsed.lease_token == lease && rows_match(&parsed.opts, expected_rows)
    })
}

#[test]
fn create_parses_binary_tuple_and_map_item_shapes() -> Result<(), Box<dyn std::error::Error>> {
    let id = Raw::Binary(b"flow-1");
    // Source L8.
    assert!(parsed_match(&many::create(&id), b"flow-1", None, &[]));
    let payload_pair = [Raw::Atom("payload"), Raw::Binary(b"p")];
    let keyword_rows = [Raw::Tuple(&payload_pair)];
    let keyword_parts = [id, Raw::List(&keyword_rows)];
    let keyword = Raw::Tuple(&keyword_parts);
    // Source L9.
    assert!(parsed_match(
        &many::create(&keyword),
        b"flow-1",
        None,
        &[("payload", &payload_pair[1])]
    ));
    let flat_ref_parts = [
        Raw::Atom("id"),
        id,
        Raw::Atom("payload_ref"),
        Raw::Binary(b"ref"),
    ];
    let flat_ref = Raw::Tuple(&flat_ref_parts);
    // Source L11.
    assert!(parsed_match(
        &many::create(&flat_ref),
        b"flow-1",
        None,
        &[("payload_ref", &flat_ref_parts[3])]
    ));
    let flat_parts = [
        Raw::Atom("id"),
        id,
        Raw::Atom("partition_key"),
        Raw::Binary(b"p"),
        Raw::Atom("payload"),
        Raw::Binary(b"payload"),
    ];
    let flat = Raw::Tuple(&flat_parts);
    // Source L14.
    assert!(parsed_match(
        &many::create(&flat),
        b"flow-1",
        None,
        &[
            ("partition_key", &flat_parts[3]),
            ("payload", &flat_parts[5])
        ]
    ));
    let map_rows = [
        (Raw::Binary(b"id"), id),
        (Raw::Binary(b"type"), Raw::Binary(b"email")),
        (Raw::Binary(b"partition_key"), Raw::Binary(b"p")),
    ];
    let map = Raw::Map(&map_rows);
    let parsed = many::create(&map);
    // Source L17: the complete result pattern, before its two field observations.
    assert!(matches!(&parsed, Ok(value) if value.id == b"flow-1" && value.lease_token.is_none()));
    let parsed = parsed?;
    // Source L20.
    assert!(matches!(
        parsed
            .opts
            .iter()
            .find(|(key, _)| *key == "type")
            .map(|(_, value)| *value),
        Some(Raw::Binary(b"email"))
    ));
    // Source L21.
    assert!(matches!(
        parsed
            .opts
            .iter()
            .find(|(key, _)| *key == "partition_key")
            .map(|(_, value)| *value),
        Some(Raw::Binary(b"p"))
    ));
    Ok(())
}

#[test]
fn terminal_item_parsers_preserve_map_values_and_lease_tuple_shapes()
-> Result<(), Box<dyn std::error::Error>> {
    let map_rows = [
        (Raw::Atom("id"), Raw::Binary(b"flow-1")),
        (Raw::Atom("lease_token"), Raw::Binary(b"lease")),
        (Raw::Atom("fencing_token"), Raw::Integer(7)),
        (Raw::Atom("result"), Raw::Binary(b"ok")),
        (Raw::Atom("payload"), Raw::Binary(b"p")),
    ];
    let map = Raw::Map(&map_rows);
    let parsed = many::complete(&map);
    // Source L26.
    assert!(matches!(&parsed, Ok(value)
        if value.id == b"flow-1" && value.lease_token == Some(b"lease".as_slice())));
    let parsed = parsed?;
    // Source L27.
    assert!(matches!(
        parsed
            .opts
            .iter()
            .find(|(key, _)| *key == "fencing_token")
            .map(|(_, value)| *value),
        Some(Raw::Integer(7))
    ));
    // Source L28.
    assert!(matches!(
        parsed
            .opts
            .iter()
            .find(|(key, _)| *key == "result")
            .map(|(_, value)| *value),
        Some(Raw::Binary(b"ok"))
    ));
    // Source L29.
    assert!(matches!(
        parsed
            .opts
            .iter()
            .find(|(key, _)| *key == "payload")
            .map(|(_, value)| *value),
        Some(Raw::Binary(b"p"))
    ));
    let retry_parts = [
        Raw::Atom("id"),
        Raw::Binary(b"flow-1"),
        Raw::Atom("lease_token"),
        Raw::Binary(b"lease"),
        Raw::Atom("fencing_token"),
        Raw::Integer(7),
    ];
    let retry = Raw::Tuple(&retry_parts);
    // Source L31.
    assert!(parsed_match(
        &many::retry(&retry),
        b"flow-1",
        Some(b"lease"),
        &[("fencing_token", &retry_parts[5])]
    ));
    let lease_pair = [Raw::Atom("lease_token"), Raw::Binary(b"lease")];
    let fence_pair = [Raw::Atom("fencing_token"), Raw::Integer(7)];
    let keyword_rows = [Raw::Tuple(&lease_pair), Raw::Tuple(&fence_pair)];
    let fail_parts = [Raw::Binary(b"flow-1"), Raw::List(&keyword_rows)];
    let fail = Raw::Tuple(&fail_parts);
    // Source L34.
    assert!(parsed_match(
        &many::fail(&fail),
        b"flow-1",
        Some(b"lease"),
        &[
            ("lease_token", &lease_pair[1]),
            ("fencing_token", &fence_pair[1])
        ]
    ));
    Ok(())
}

#[test]
fn cancel_and_transition_preserve_optional_lease_and_partition_keys()
-> Result<(), Box<dyn std::error::Error>> {
    let string_rows = [
        (Raw::Binary(b"id"), Raw::Binary(b"flow-1")),
        (Raw::Binary(b"fencing_token"), Raw::Integer(3)),
        (Raw::Binary(b"partition_key"), Raw::Binary(b"p")),
    ];
    let string_map = Raw::Map(&string_rows);
    // Source L39.
    assert!(parsed_match(
        &many::cancel(&string_map),
        b"flow-1",
        None,
        &[
            ("fencing_token", &string_rows[1].1),
            ("partition_key", &string_rows[2].1)
        ]
    ));
    let meta = [(Raw::Binary(b"version"), Raw::Integer(4))];
    let attributes = [(Raw::Binary(b"phase"), Raw::Binary(b"cancelled"))];
    let atom_rows = [
        (Raw::Atom("id"), Raw::Binary(b"flow-1")),
        (Raw::Atom("fencing_token"), Raw::Integer(3)),
        (Raw::Atom("state_meta"), Raw::Map(&meta)),
        (Raw::Atom("attributes_merge"), Raw::Map(&attributes)),
    ];
    let atom_map = Raw::Map(&atom_rows);
    let parsed = many::cancel(&atom_map);
    // Source L42.
    assert!(matches!(&parsed, Ok(value) if value.id == b"flow-1" && value.lease_token.is_none()));
    let parsed = parsed?;
    // Source L50.
    assert!(matches!(
        parsed.opts.iter().find(|(key, _)| *key == "state_meta").map(|(_, value)| *value),
        Some(Raw::Map(rows)) if matches!(rows, [(Raw::Binary(b"version"), Raw::Integer(4))])
    ));
    // Source L51.
    assert!(matches!(
        parsed.opts.iter().find(|(key, _)| *key == "attributes_merge").map(|(_, value)| *value),
        Some(Raw::Map(rows))
            if matches!(rows, [(Raw::Binary(b"phase"), Raw::Binary(b"cancelled"))])
    ));
    let transition_parts = [
        Raw::Atom("id"),
        Raw::Binary(b"flow-1"),
        Raw::Atom("partition_key"),
        Raw::Binary(b"p"),
        Raw::Atom("fencing_token"),
        Raw::Integer(3),
        Raw::Atom("lease_token"),
        Raw::Nil,
    ];
    let transition = Raw::Tuple(&transition_parts);
    // Source L53.
    assert!(parsed_match(
        &many::transition(&transition),
        b"flow-1",
        None,
        &[
            ("partition_key", &transition_parts[3]),
            ("fencing_token", &transition_parts[5])
        ]
    ));
    Ok(())
}

#[test]
fn merge_applies_item_opts_over_base_opts_and_pins_partition_key()
-> Result<(), Box<dyn std::error::Error>> {
    let workflow_type = Raw::Binary(b"email");
    let base_partition = Raw::Binary(b"base");
    let state = Raw::Binary(b"queued");
    let partition = Raw::Binary(b"item");
    let base = [("type", &workflow_type), ("partition_key", &base_partition)];
    let item = [("state", &state)];
    let opts = many::merge(&base, &item, &partition)?;
    // Source L62.
    assert!(matches!(
        opts.iter()
            .find(|(key, _)| *key == "type")
            .map(|(_, value)| *value),
        Some(Raw::Binary(b"email"))
    ));
    // Source L63.
    assert!(matches!(
        opts.iter()
            .find(|(key, _)| *key == "state")
            .map(|(_, value)| *value),
        Some(Raw::Binary(b"queued"))
    ));
    // Source L64.
    assert!(matches!(
        opts.iter()
            .find(|(key, _)| *key == "partition_key")
            .map(|(_, value)| *value),
        Some(Raw::Binary(b"item"))
    ));
    Ok(())
}

#[test]
fn invalid_item_opts_keep_existing_error_messages() {
    let bad_rows = [Raw::Atom("bad")];
    let create_parts = [Raw::Binary(b"flow-1"), Raw::List(&bad_rows)];
    let create = Raw::Tuple(&create_parts);
    // Source L68.
    assert!(matches!(many::create(&create), Err(error)
        if error.to_string() == "ERR flow opts must be a keyword list"));
    let fence_pair = [Raw::Atom("fencing_token"), Raw::Integer(1)];
    let keyword_rows = [Raw::Tuple(&fence_pair)];
    let complete_parts = [Raw::Binary(b"flow-1"), Raw::List(&keyword_rows)];
    let complete = Raw::Tuple(&complete_parts);
    // Source L71.
    assert!(matches!(many::complete(&complete), Err(error)
        if error.to_string() == "ERR flow lease_token must be a non-empty string"));
    // Source L74.
    assert!(matches!(many::transition(&Raw::Atom("bad")), Err(error)
        if error.to_string() == "ERR flow id must be a non-empty string"));
}

#[test]
fn item_parsers_reject_empty_ids_and_required_lease_tokens_at_the_boundary() {
    // Source L78.
    assert!(matches!(many::create(&Raw::Binary(b"")), Err(error)
        if error.to_string() == "ERR flow id must be a non-empty string"));
    let create_rows = [
        (Raw::Atom("id"), Raw::Binary(b"")),
        (Raw::Atom("type"), Raw::Binary(b"email")),
    ];
    let create = Raw::Map(&create_rows);
    // Source L80.
    assert!(matches!(many::create(&create), Err(error)
        if error.to_string() == "ERR flow id must be a non-empty string"));
    let fence_pair = [Raw::Atom("fencing_token"), Raw::Integer(1)];
    let keyword_rows = [Raw::Tuple(&fence_pair)];
    let complete_parts = [
        Raw::Binary(b"flow-1"),
        Raw::Binary(b""),
        Raw::List(&keyword_rows),
    ];
    let complete = Raw::Tuple(&complete_parts);
    // Source L83.
    assert!(matches!(many::complete(&complete), Err(error)
        if error.to_string() == "ERR flow lease_token must be a non-empty string"));
    let map_rows = [
        (Raw::Atom("id"), Raw::Binary(b"flow-1")),
        (Raw::Atom("lease_token"), Raw::Binary(b"")),
        (Raw::Atom("fencing_token"), Raw::Integer(1)),
    ];
    let map = Raw::Map(&map_rows);
    // Source L86.
    assert!(matches!(many::complete(&map), Err(error)
        if error.to_string() == "ERR flow lease_token must be a non-empty string"));
    // Source L89.
    assert!(matches!(many::cancel(&map), Err(error)
        if error.to_string() == "ERR flow lease_token must be a non-empty string"));
    let transition_parts = [
        Raw::Atom("id"),
        Raw::Binary(b"flow-1"),
        Raw::Atom("fencing_token"),
        Raw::Integer(1),
        Raw::Atom("lease_token"),
        Raw::Binary(b""),
    ];
    let transition = Raw::Tuple(&transition_parts);
    // Source L92.
    assert!(matches!(many::transition(&transition), Err(error)
        if error.to_string() == "ERR flow lease_token must be a non-empty string"));
}
