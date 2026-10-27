use super::{ManyItemOptionsError, Raw, many, parsed_match, rows_match};
use ManyItemOptionsError::{Id, LeaseToken, Options};
use Raw::{Atom, Binary, Integer, List, Map, Nil, Other, Tuple};

#[test]
fn core_map_families_and_optional_atom_presence_follow_source_precedence() {
    let lease_a = Some(b"lease-a".as_slice());
    let lease_b = Some(b"lease-b".as_slice());
    let mut rows = [
        (Atom("id"), Binary(b"id-a")),
        (Atom("lease_token"), Binary(b"lease-a")),
        (Atom("fencing_token"), Other),
        (Binary(b"id"), Binary(b"id-b")),
        (Binary(b"lease_token"), Binary(b"lease-b")),
        (Binary(b"fencing_token"), Integer(9)),
        (Atom("partition_key"), Nil),
        (Binary(b"partition_key"), Binary(b"shadowed")),
        (Atom("unknown"), Other),
    ];
    let item = Map(&rows);
    let expected = [("partition_key", &rows[6].1)];
    assert!(parsed_match(&many::create(&item), b"id-a", None, &expected));
    let expected = [("fencing_token", &rows[2].1), ("partition_key", &rows[6].1)];
    for parse in [many::complete, many::retry, many::fail] {
        assert!(parsed_match(&parse(&item), b"id-a", lease_a, &expected));
    }
    let expected = [
        ("fencing_token", &rows[2].1),
        ("lease_token", &rows[1].1),
        ("partition_key", &rows[6].1),
    ];
    for parse in [many::cancel, many::transition] {
        assert!(parsed_match(&parse(&item), b"id-a", None, &expected));
    }
    rows[0].1 = Other;
    let item = Map(&rows);
    let expected = [("partition_key", &rows[6].1)];
    assert!(parsed_match(&many::create(&item), b"id-b", None, &expected));
    let expected = [("fencing_token", &rows[5].1), ("partition_key", &rows[6].1)];
    for parse in [many::complete, many::retry, many::fail] {
        assert!(parsed_match(&parse(&item), b"id-b", lease_b, &expected));
    }
    let expected = [
        ("fencing_token", &rows[5].1),
        ("lease_token", &rows[1].1),
        ("partition_key", &rows[6].1),
    ];
    for parse in [many::cancel, many::transition] {
        assert!(parsed_match(&parse(&item), b"id-b", None, &expected));
    }
    rows[0].1 = Binary(b"id-a");
    rows[2].0 = Atom("missing_atom_fence");
    let item = Map(&rows);
    let expected = [("fencing_token", &rows[5].1), ("partition_key", &rows[6].1)];
    for parse in [many::complete, many::retry, many::fail] {
        assert!(parsed_match(&parse(&item), b"id-b", lease_b, &expected));
    }
    rows[2].0 = Atom("fencing_token");
    rows[0].1 = Binary(b"");
    let item = Map(&rows);
    for parse in [
        many::create,
        many::complete,
        many::retry,
        many::fail,
        many::cancel,
        many::transition,
    ] {
        assert!(matches!(parse(&item), Err(Id)));
    }
    rows[0].1 = Binary(b"id-a");
    rows[1].1 = Other;
    let item = Map(&rows);
    let expected = [("partition_key", &rows[6].1)];
    assert!(parsed_match(&many::create(&item), b"id-a", None, &expected));
    for parse in [
        many::complete,
        many::retry,
        many::fail,
        many::cancel,
        many::transition,
    ] {
        assert!(matches!(parse(&item), Err(LeaseToken)));
    }
    let mixed = [
        (Atom("id"), Binary(b"id")),
        (Binary(b"lease_token"), Binary(b"lease")),
        (Atom("fencing_token"), Other),
    ];
    let item = Map(&mixed);
    for parse in [many::complete, many::retry, many::fail] {
        assert!(matches!(parse(&item), Err(Id)));
    }
}

#[test]
fn keyword_lease_validation_and_shape_guards_preserve_error_order() {
    let prefix = [Atom("lease_token"), Other];
    let bad_rows = [
        Atom("bad"),
        Tuple(&[Binary(b"lease_token"), Nil]),
        Tuple(&[Atom("lease_token"), Nil, Other]),
    ];
    for bad_row in bad_rows {
        let bad = [Tuple(&prefix), bad_row];
        for (id, lease, two_error, three_error) in [
            (Binary(b""), Binary(b""), Id, Id),
            (Binary(b"id"), Binary(b""), Options, LeaseToken),
            (Binary(b"id"), Other, Options, Id),
            (Binary(b"id"), Binary(b"lease"), Options, Options),
        ] {
            let parts = [id, List(&bad)];
            let item = Tuple(&parts);
            for parse in [
                many::create,
                many::complete,
                many::retry,
                many::fail,
                many::cancel,
                many::transition,
            ] {
                assert!(matches!(parse(&item), Err(error)
                    if error.to_string() == two_error.to_string()));
            }
            let parts = [id, lease, List(&bad)];
            let item = Tuple(&parts);
            for parse in [many::complete, many::retry, many::fail] {
                assert!(matches!(parse(&item), Err(error)
                    if error.to_string() == three_error.to_string()));
            }
        }
    }
    assert_keyword_lease_shapes();
}

fn assert_keyword_lease_shapes() {
    for lease in [Other, Binary(b""), Nil, Binary(b"lease")] {
        let parts = [
            Atom("id"),
            Binary(b"id"),
            Atom("lease_token"),
            lease,
            Atom("fencing_token"),
            Other,
        ];
        let item = Tuple(&parts);
        for parse in [many::complete, many::retry, many::fail] {
            if matches!(lease, Binary(b"lease")) {
                let expected = [("fencing_token", &parts[5])];
                assert!(parsed_match(
                    &parse(&item),
                    b"id",
                    Some(b"lease"),
                    &expected
                ));
            } else {
                assert!(matches!(parse(&item), Err(LeaseToken)));
            }
        }
    }
    for first in [Other, Binary(b""), Nil, Atom("nil")] {
        let first_pair = [Atom("lease_token"), first];
        let later_pair = [Atom("lease_token"), Binary(b"later")];
        let rows = [Tuple(&first_pair), Tuple(&later_pair)];
        let parts = [Binary(b"id"), List(&rows)];
        let item = Tuple(&parts);
        for parse in [many::complete, many::retry, many::fail] {
            assert!(matches!(parse(&item), Err(LeaseToken)));
        }
        let expected = [
            ("lease_token", &first_pair[1]),
            ("lease_token", &later_pair[1]),
        ];
        for parse in [many::create, many::cancel, many::transition] {
            if matches!(first, Nil | Atom("nil")) {
                assert!(parsed_match(&parse(&item), b"id", None, &expected));
            } else {
                assert!(matches!(parse(&item), Err(LeaseToken)));
            }
        }
        let parts = [Binary(b"id"), Binary(b"external"), List(&rows)];
        let item = Tuple(&parts);
        for parse in [many::complete, many::retry, many::fail] {
            assert!(parsed_match(
                &parse(&item),
                b"id",
                Some(b"external"),
                &expected
            ));
        }
    }
    for error in [Id, Options, LeaseToken] {
        assert!(std::error::Error::source(&error).is_none());
    }
}

#[test]
fn merge_preserves_duplicates_and_pins_any_partition_descriptor()
-> Result<(), Box<dyn std::error::Error>> {
    let donors = [Other, Nil, Binary(b"old"), Binary(b"new"), Integer(9)];
    let base_key = String::from("keep");
    let item_key = String::from("replace");
    for partition in [Nil, Other] {
        let merged = {
            let base = [
                (base_key.as_str(), &donors[0]),
                ("partition_key", &donors[2]),
                ("replace", &donors[2]),
                (base_key.as_str(), &donors[1]),
                ("partition_key", &donors[1]),
                ("replace", &donors[0]),
            ];
            let item = [
                ("partition_key", &donors[4]),
                (item_key.as_str(), &donors[3]),
                (item_key.as_str(), &donors[1]),
                ("partition_key", &donors[0]),
            ];
            many::merge(&base, &item, &partition)?
        };
        let expected = [
            ("partition_key", &partition),
            ("keep", &donors[0]),
            ("keep", &donors[1]),
            ("replace", &donors[3]),
            ("replace", &donors[1]),
        ];
        assert!(rows_match(&merged, &expected));
        assert_eq!(merged[1].0.as_ptr(), base_key.as_ptr());
        assert_eq!(merged[3].0.as_ptr(), item_key.as_ptr());
        let base = [
            ("keep", &donors[0]),
            ("partition_key", &donors[2]),
            ("keep", &donors[1]),
            ("partition_key", &donors[0]),
        ];
        let expected = [
            ("partition_key", &partition),
            ("keep", &donors[0]),
            ("keep", &donors[1]),
        ];
        assert!(rows_match(&many::merge(&base, &[], &partition)?, &expected));
        let expected = [("partition_key", &partition)];
        assert!(rows_match(&many::merge(&[], &[], &partition)?, &expected));
        assert!(matches!(
            donors,
            [Other, Nil, Binary(b"old"), Binary(b"new"), Integer(9)]
        ));
    }
    Ok(())
}

#[test]
fn parsed_many_items_borrow_ids_leases_and_opaque_option_values()
-> Result<(), Box<dyn std::error::Error>> {
    let mut id_bytes = vec![b' '; 4097];
    id_bytes[1] = 0;
    id_bytes[2] = 255;
    let lease_bytes = [b' ', 254, 0, b' '];
    let id = id_bytes.as_slice();
    let lease = Some(lease_bytes.as_slice());
    let unknown = String::from("unrecognized caller atom");
    let opaque_rows = [(Other, Tuple(&[Nil, Other]))];
    let pairs = [
        [Atom("lease_token"), Binary(&lease_bytes)],
        [Atom("lease_token"), Other],
        [Atom(&unknown), Other],
        [Nil, Map(&opaque_rows)],
        [Atom("nil"), Nil],
    ];
    let rows: Vec<_> = pairs.iter().map(|pair| Tuple(pair)).collect();
    let parts = [Binary(&id_bytes), List(&rows)];
    let item = Tuple(&parts);
    let expected = [
        ("lease_token", &pairs[0][1]),
        ("lease_token", &pairs[1][1]),
        (unknown.as_str(), &pairs[2][1]),
        ("nil", &pairs[3][1]),
        ("nil", &pairs[4][1]),
    ];
    for parse in [many::create, many::cancel, many::transition] {
        assert!(parsed_match(&parse(&item), id, None, &expected));
    }
    for parse in [many::complete, many::retry, many::fail] {
        assert!(parsed_match(&parse(&item), id, lease, &expected));
    }
    let parsed = many::complete(&item)?;
    assert_eq!(parsed.id.as_ptr(), id_bytes.as_ptr());
    assert!(
        parsed
            .lease_token
            .is_some_and(|lease| lease.as_ptr() == lease_bytes.as_ptr())
    );
    assert_eq!(parsed.opts.len(), 5);
    assert_eq!(parsed.opts[2].0.as_ptr(), unknown.as_ptr());
    assert!(std::ptr::eq(parsed.opts[3].1, &raw const pairs[3][1]));
    assert!(matches!(pairs[1][1], Other));
    assert!(matches!(parts[0], Binary(bytes) if std::ptr::eq(bytes, id_bytes.as_slice())));
    assert!(matches!(pairs[0][1], Binary(bytes) if std::ptr::eq(bytes, lease_bytes.as_slice())));
    let short = [
        Atom("id"),
        Binary(&id_bytes),
        Atom("fencing_token"),
        Other,
        Atom("lease_token"),
        Binary(&lease_bytes),
    ];
    let item = Tuple(&short);
    let expected = [("fencing_token", &short[3]), ("lease_token", &short[5])];
    assert!(parsed_match(&many::transition(&item), id, None, &expected));
    let item = Tuple(&short[..4]);
    let expected = [("fencing_token", &short[3])];
    assert!(parsed_match(&many::cancel(&item), id, None, &expected));
    let long = [
        Atom("id"),
        Binary(&id_bytes),
        Atom("partition_key"),
        Other,
        Atom("fencing_token"),
        Other,
        Atom("lease_token"),
        Binary(&lease_bytes),
    ];
    let item = Tuple(&long);
    let expected = [
        ("partition_key", &long[3]),
        ("fencing_token", &long[5]),
        ("lease_token", &long[7]),
    ];
    assert!(parsed_match(&many::transition(&item), id, None, &expected));
    let item = Tuple(&long[..6]);
    let expected = [("partition_key", &long[3]), ("fencing_token", &long[5])];
    assert!(parsed_match(&many::cancel(&item), id, None, &expected));
    Ok(())
}
