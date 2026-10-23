use super::ManyItemOptionsError::Id;
use super::{Raw, many, parsed_match};
use Raw::{Atom, Binary, Integer, List, Map, Nil, Other, Tuple};

fn dense_map() -> Vec<(Raw<'static>, Raw<'static>)> {
    vec![
        (Atom("id"), Binary(b"id")),
        (Atom("lease_token"), Binary(b"l")),
        (Atom("fencing_token"), Integer(7)),
        (Atom("type"), Other),
        (Atom("state"), Other),
        (Atom("run_at_ms"), Other),
        (Atom("priority"), Other),
        (Atom("payload"), Other),
        (Atom("payload_ref"), Other),
        (Atom("values"), Other),
        (Atom("value_refs"), Other),
        (Atom("attributes"), Other),
        (Atom("state_meta"), Other),
        (Atom("partition_key"), Other),
        (Atom("parent_flow_id"), Other),
        (Atom("root_flow_id"), Other),
        (Atom("correlation_id"), Other),
        (Atom("idempotent"), Other),
        (Atom("retention_ttl_ms"), Other),
        (Atom("max_active_ms"), Other),
        (Atom("history_hot_max_events"), Other),
        (Atom("history_max_events"), Other),
        (Atom("drop_values"), Other),
        (Atom("override_values"), Other),
        (Atom("attributes_merge"), Other),
        (Atom("attributes_delete"), Other),
        (Atom("result_ref"), Other),
        (Atom("result"), Other),
        (Atom("error_ref"), Other),
        (Atom("error"), Other),
        (Atom("retry"), Other),
        (Atom("reason_ref"), Other),
        (Atom("reason"), Other),
    ]
}

#[test]
fn create_map_options_follow_source_prepend_order() {
    for binary_keys in [false, true] {
        let mut rows = dense_map();
        if binary_keys {
            for (key, _) in &mut rows {
                if let Atom(name) = key {
                    *key = Binary(name.as_bytes());
                }
            }
        }
        let item = Map(&rows);
        let expected = [
            ("history_max_events", &rows[21].1),
            ("history_hot_max_events", &rows[20].1),
            ("max_active_ms", &rows[19].1),
            ("retention_ttl_ms", &rows[18].1),
            ("idempotent", &rows[17].1),
            ("correlation_id", &rows[16].1),
            ("root_flow_id", &rows[15].1),
            ("parent_flow_id", &rows[14].1),
            ("partition_key", &rows[13].1),
            ("state_meta", &rows[12].1),
            ("attributes", &rows[11].1),
            ("value_refs", &rows[10].1),
            ("values", &rows[9].1),
            ("payload_ref", &rows[8].1),
            ("payload", &rows[7].1),
            ("priority", &rows[6].1),
            ("run_at_ms", &rows[5].1),
            ("state", &rows[4].1),
            ("type", &rows[3].1),
        ];
        assert!(parsed_match(&many::create(&item), b"id", None, &expected));
    }
    let payload = [Atom("id"), Binary(b"id"), Atom("payload"), Other];
    let item = Tuple(&payload);
    let expected = [("payload", &payload[3])];
    assert!(parsed_match(&many::create(&item), b"id", None, &expected));
    let reference = [
        Atom("id"),
        Binary(b"id"),
        Atom("partition_key"),
        Nil,
        Atom("payload_ref"),
        Other,
    ];
    let item = Tuple(&reference);
    let expected = [
        ("partition_key", &reference[3]),
        ("payload_ref", &reference[5]),
    ];
    assert!(parsed_match(&many::create(&item), b"id", None, &expected));
    for label in [Atom("unknown"), Binary(b"payload"), Binary(b"payload_ref")] {
        let parts = [Atom("id"), Binary(b"id"), label, Other];
        assert!(matches!(many::create(&Tuple(&parts)), Err(Id)));
    }
    let parts = [Binary(b"id"), Binary(b"id"), Atom("payload"), Other];
    assert!(matches!(many::create(&Tuple(&parts)), Err(Id)));
    for root in [
        Nil,
        Other,
        Atom("bad"),
        List(&[]),
        Tuple(&[]),
        Map(&[]),
        Tuple(&[Binary(b"id"), Nil]),
        Tuple(&[Other, List(&[])]),
    ] {
        for parse in [
            many::create,
            many::complete,
            many::retry,
            many::fail,
            many::cancel,
            many::transition,
        ] {
            assert!(matches!(parse(&root), Err(Id)));
        }
    }
}

#[test]
fn terminal_map_option_groups_preserve_source_order() {
    let lease = Some(b"l".as_slice());
    for binary_keys in [false, true] {
        let mut rows = dense_map();
        if binary_keys {
            for (key, _) in &mut rows {
                if let Atom(name) = key {
                    *key = Binary(name.as_bytes());
                }
            }
        }
        let item = Map(&rows);
        let expected = [
            ("fencing_token", &rows[2].1),
            ("result_ref", &rows[26].1),
            ("result", &rows[27].1),
            ("state_meta", &rows[12].1),
            ("attributes_delete", &rows[25].1),
            ("attributes_merge", &rows[24].1),
            ("attributes", &rows[11].1),
            ("override_values", &rows[23].1),
            ("drop_values", &rows[22].1),
            ("value_refs", &rows[10].1),
            ("values", &rows[9].1),
            ("payload_ref", &rows[8].1),
            ("payload", &rows[7].1),
            ("partition_key", &rows[13].1),
        ];
        assert!(parsed_match(
            &many::complete(&item),
            b"id",
            lease,
            &expected
        ));
        let expected = [
            ("fencing_token", &rows[2].1),
            ("error_ref", &rows[28].1),
            ("error", &rows[29].1),
            ("state_meta", &rows[12].1),
            ("attributes_delete", &rows[25].1),
            ("attributes_merge", &rows[24].1),
            ("attributes", &rows[11].1),
            ("override_values", &rows[23].1),
            ("drop_values", &rows[22].1),
            ("value_refs", &rows[10].1),
            ("values", &rows[9].1),
            ("payload_ref", &rows[8].1),
            ("payload", &rows[7].1),
            ("retry", &rows[30].1),
            ("partition_key", &rows[13].1),
        ];
        assert!(parsed_match(&many::retry(&item), b"id", lease, &expected));
        let expected = [
            ("fencing_token", &rows[2].1),
            ("error_ref", &rows[28].1),
            ("error", &rows[29].1),
            ("state_meta", &rows[12].1),
            ("attributes_delete", &rows[25].1),
            ("attributes_merge", &rows[24].1),
            ("attributes", &rows[11].1),
            ("override_values", &rows[23].1),
            ("drop_values", &rows[22].1),
            ("value_refs", &rows[10].1),
            ("values", &rows[9].1),
            ("payload_ref", &rows[8].1),
            ("payload", &rows[7].1),
            ("partition_key", &rows[13].1),
        ];
        assert!(parsed_match(&many::fail(&item), b"id", lease, &expected));
    }
    let parts = [
        Atom("id"),
        Binary(b"id"),
        Atom("partition_key"),
        Nil,
        Atom("lease_token"),
        Binary(b"l"),
        Atom("fencing_token"),
        Other,
    ];
    let item = Tuple(&parts);
    let expected = [("partition_key", &parts[3]), ("fencing_token", &parts[7])];
    for parse in [many::complete, many::retry, many::fail] {
        assert!(parsed_match(&parse(&item), b"id", lease, &expected));
    }
    let bare = Binary(b"id");
    for parse in [
        many::complete,
        many::retry,
        many::fail,
        many::cancel,
        many::transition,
    ] {
        assert!(matches!(parse(&bare), Err(Id)));
    }
}

#[test]
fn cancel_transition_map_and_flat_lease_presence_remain_distinct() {
    for binary_keys in [false, true] {
        let mut rows = dense_map();
        if binary_keys {
            for (key, _) in &mut rows {
                if let Atom(name) = key {
                    *key = Binary(name.as_bytes());
                }
            }
        }
        let item = Map(&rows);
        let expected = [
            ("fencing_token", &rows[2].1),
            ("lease_token", &rows[1].1),
            ("reason_ref", &rows[31].1),
            ("state_meta", &rows[12].1),
            ("attributes_delete", &rows[25].1),
            ("attributes_merge", &rows[24].1),
            ("attributes", &rows[11].1),
            ("override_values", &rows[23].1),
            ("drop_values", &rows[22].1),
            ("value_refs", &rows[10].1),
            ("values", &rows[9].1),
            ("reason", &rows[32].1),
            ("partition_key", &rows[13].1),
        ];
        assert!(parsed_match(&many::cancel(&item), b"id", None, &expected));
        let expected = [
            ("fencing_token", &rows[2].1),
            ("state_meta", &rows[12].1),
            ("attributes_delete", &rows[25].1),
            ("attributes_merge", &rows[24].1),
            ("attributes", &rows[11].1),
            ("override_values", &rows[23].1),
            ("drop_values", &rows[22].1),
            ("value_refs", &rows[10].1),
            ("values", &rows[9].1),
            ("payload_ref", &rows[8].1),
            ("payload", &rows[7].1),
            ("lease_token", &rows[1].1),
            ("partition_key", &rows[13].1),
        ];
        assert!(parsed_match(
            &many::transition(&item),
            b"id",
            None,
            &expected
        ));
    }
    for lease in [Nil, Atom("nil")] {
        let rows = [
            (Binary(b"id"), Binary(b"id")),
            (Binary(b"fencing_token"), Other),
            (Atom("lease_token"), lease),
            (Binary(b"lease_token"), Binary(b"shadowed")),
        ];
        let item = Map(&rows);
        let expected = [("fencing_token", &rows[1].1), ("lease_token", &rows[2].1)];
        for parse in [many::cancel, many::transition] {
            assert!(parsed_match(&parse(&item), b"id", None, &expected));
        }
        let parts = [
            Atom("id"),
            Binary(b"id"),
            Atom("fencing_token"),
            Other,
            Atom("lease_token"),
            lease,
        ];
        let item = Tuple(&parts);
        let expected = [("fencing_token", &parts[3])];
        assert!(parsed_match(
            &many::transition(&item),
            b"id",
            None,
            &expected
        ));
    }
    for label in [Atom("unknown"), Binary(b"fencing_token")] {
        let parts = [Atom("id"), Binary(b"id"), label, Other];
        assert!(matches!(many::cancel(&Tuple(&parts)), Err(Id)));
    }
}
