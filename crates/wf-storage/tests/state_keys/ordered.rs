use super::{TestResult, entry, hex, history, lane, record, scope, standard_scope};
use wf_domain::claim::{DueOrder, LaneKey, LaneOrder};
use wf_domain::workflow::request::RequestScope;
use wf_storage::state::key::{
    DecodedOrderedKey, decode_due_key, decode_fifo_key, decode_history_key, encode_due_key,
    encode_due_prefix, encode_fifo_key, encode_fifo_prefix, encode_history_key,
    encode_history_prefix,
};
use wf_storage::state::value::StateValueError;
use wf_types::identity::{PartitionId, RecordId};
use wf_types::version::TimestampMillis;

// Source grammar packed before SUT: binary i/NUL/FF, full2^64+3 sequence.
const DUE: &str =
    "4657504b0001036700007a00007400000000000000000000016900ffff0000700000710000720000";
const FIFO: &str = "4657504b0001046700007a000074000072000071000070000000000000000000000100000000000000036900ffff0000";
const HISTORY: &str =
    "4657504b0001056700007a000074000070000069000000000000000000643130302d31300000";
const RAW: [&[u8]; 11] = [
    b"", b"\0", b"\0\0", b"\0\xff", b"\x01", b"a", b"a\0", b"a\0\xff", b"aa", b"\xff", b"\xff\0",
];

fn due() -> DueOrder<'static> {
    DueOrder {
        priority: 2,
        due: TimestampMillis::new(1),
        id: b"i\0\xff",
        partition_key: b"p",
        state: b"q",
        workflow_type: b"r",
    }
}

fn decode_for(
    family: u8,
    bytes: &[u8],
    scope: &RequestScope,
    wire_cap: usize,
    decoded_cap: usize,
) -> Result<DecodedOrderedKey, StateValueError> {
    match family {
        3 => decode_due_key(bytes, scope, wire_cap, decoded_cap),
        4 => decode_fifo_key(bytes, scope, lane(), wire_cap, decoded_cap),
        5 => {
            let partition =
                PartitionId::new(b"p".to_vec()).map_err(|_| StateValueError::InvalidValue)?;
            let id = RecordId::new(b"i".to_vec()).map_err(|_| StateValueError::InvalidValue)?;
            decode_history_key(bytes, scope, &partition, &id, wire_cap, decoded_cap)
        }
        _ => Err(StateValueError::UnsupportedTag),
    }
}

#[test]
fn ordered_key_vectors_match_due_fifo_and_history() -> TestResult {
    let scope = standard_scope()?;
    let running = record(b"i\0\xff", true)?;
    let entry = entry(&running, (1_u128 << 64) + 3);
    let member = LaneOrder::for_record(&entry);
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    assert_eq!(member.id(), b"i\0\xff");
    assert_eq!(member.sequence(), (1_u128 << 64) + 3);
    assert_eq!(member.score(), -1);
    assert_eq!(encode_due_key(&scope, due(), 40)?, hex(DUE)?);
    assert_eq!(encode_fifo_key(&scope, lane(), member, 48)?, hex(FIFO)?);
    assert_eq!(
        encode_history_key(&scope, &partition, &id, &history("100-10", 10), 38)?,
        hex(HISTORY)?
    );
    assert_eq!(
        encode_due_prefix(&scope, 16)?,
        hex("4657504b0001036700007a0000740000")?
    );
    assert_eq!(
        encode_fifo_prefix(&scope, lane(), 25)?,
        hex("4657504b0001046700007a0000740000720000710000700000")?
    );
    assert_eq!(
        encode_history_prefix(&scope, &partition, &id, 22)?,
        hex("4657504b0001056700007a0000740000700000690000")?
    );
    let bytes = hex(DUE)?;
    let owner: DecodedOrderedKey = decode_due_key(&bytes, &scope, 40, 134)?;
    drop(bytes);
    assert_eq!(owner.due(), Some(due()));
    assert_eq!(owner.logical_bytes(), 134);
    assert!(
        owner.lane().is_none() && owner.member().is_none() && owner.history_position().is_none()
    );
    let bytes = hex(FIFO)?;
    let owner: DecodedOrderedKey = decode_fifo_key(&bytes, &scope, lane(), 48, 134)?;
    drop(bytes);
    assert_eq!(owner.lane(), Some(lane()));
    assert_eq!(
        owner.member(),
        Some((-1, (1_u128 << 64) + 3, b"i\0\xff".as_slice()))
    );
    assert_eq!(owner.logical_bytes(), 134);
    assert!(owner.due().is_none() && owner.history_position().is_none());
    let bytes = hex(HISTORY)?;
    let owner: DecodedOrderedKey = decode_history_key(&bytes, &scope, &partition, &id, 38, 134)?;
    drop(bytes);
    assert_eq!(owner.history_position(), Some((100, "100-10")));
    assert_eq!(owner.logical_bytes(), 134);
    assert!(owner.due().is_none() && owner.lane().is_none() && owner.member().is_none());
    Ok(())
}

fn assert_due_order() -> TestResult {
    let scope = standard_scope()?;
    let base = due();
    let mut semantic = Vec::new();
    for priority in [0, 1, 2] {
        for milliseconds in [0, 1, u64::MAX] {
            semantic.push(DueOrder {
                priority,
                due: TimestampMillis::new(milliseconds),
                ..base
            });
        }
    }
    for raw in RAW {
        semantic.extend([
            DueOrder { id: raw, ..base },
            DueOrder {
                partition_key: raw,
                ..base
            },
            DueOrder { state: raw, ..base },
            DueOrder {
                workflow_type: raw,
                ..base
            },
        ]);
    }
    let mut physical = Vec::new();
    for row in &semantic {
        let key = encode_due_key(&scope, *row, 200)?;
        assert!(key.starts_with(&encode_due_prefix(&scope, 16)?));
        let owner: DecodedOrderedKey = decode_due_key(&key, &scope, 200, 200)?;
        assert_eq!(owner.due(), Some(*row));
        physical.push((key, *row));
    }
    semantic.sort();
    physical.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(
        physical.into_iter().map(|(_, row)| row).collect::<Vec<_>>(),
        semantic
    );
    Ok(())
}

fn assert_fifo_order() -> TestResult {
    let scope = standard_scope()?;
    let mut records = Vec::new();
    for running in [false, true] {
        for id in [b"a".as_slice(), b"a\0", b"a\0\xff", b"b", b"\xff"] {
            records.push(record(id, running)?);
        }
    }
    let mut semantic = Vec::new();
    for record in &records {
        for sequence in [0, (1_u128 << 53) + 1, (1_u128 << 64) + 3, u128::MAX] {
            let entry = entry(record, sequence);
            let member = LaneOrder::for_record(&entry);
            assert_eq!(member.sequence(), sequence);
            assert_eq!(member.id(), record.id().as_bytes());
            semantic.push((lane(), member));
        }
    }
    let member = semantic[0].1;
    for raw in RAW {
        semantic.extend([
            (
                LaneKey {
                    workflow_type: raw,
                    ..lane()
                },
                member,
            ),
            (
                LaneKey {
                    state: raw,
                    ..lane()
                },
                member,
            ),
            (
                LaneKey {
                    partition_key: raw,
                    ..lane()
                },
                member,
            ),
        ]);
    }
    let mut physical = Vec::new();
    for (lane, member) in &semantic {
        let key = encode_fifo_key(&scope, *lane, *member, 200)?;
        assert!(key.starts_with(&encode_fifo_prefix(&scope, *lane, 200)?));
        let owner: DecodedOrderedKey = decode_fifo_key(&key, &scope, *lane, 200, 200)?;
        assert_eq!(owner.lane(), Some(*lane));
        assert_eq!(
            owner.member(),
            Some((member.score(), member.sequence(), member.id()))
        );
        physical.push((key, (*lane, *member)));
    }
    semantic.sort();
    physical.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(
        physical.into_iter().map(|(_, row)| row).collect::<Vec<_>>(),
        semantic
    );
    Ok(())
}

fn assert_history_order() -> TestResult {
    let scope = standard_scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut keys = Vec::new();
    for (event_id, version) in [("1000-2", 2), ("999-1", 1), ("100-2", 2), ("100-10", 10)] {
        let mut event = history(event_id, version);
        let original = encode_history_key(&scope, &partition, &id, &event, 100)?;
        event.occurred_at = TimestampMillis::new(u64::MAX);
        assert_eq!(
            encode_history_key(&scope, &partition, &id, &event, 100)?,
            original
        );
        keys.push((original, event_id));
    }
    keys.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(
        keys.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
        ["100-10", "100-2", "999-1", "1000-2"]
    );
    for (key, expected) in keys {
        let owner: DecodedOrderedKey = decode_history_key(&key, &scope, &partition, &id, 100, 200)?;
        assert_eq!(owner.history_position().map(|(_, id)| id), Some(expected));
    }
    for (text, version, milliseconds) in [
        ("0-0", 0, 0),
        (
            "18446744073709551615-18446744073709551615",
            u64::MAX,
            u64::MAX,
        ),
    ] {
        let key = encode_history_key(&scope, &partition, &id, &history(text, version), 100)?;
        let owner: DecodedOrderedKey = decode_history_key(&key, &scope, &partition, &id, 100, 200)?;
        assert_eq!(owner.history_position(), Some((milliseconds, text)));
    }
    Ok(())
}

#[test]
fn ordered_key_comparison_preserves_domain_and_original_id() -> TestResult {
    assert_due_order()?;
    assert_fifo_order()?;
    assert_history_order()
}

fn assert_decoder_refusals() -> TestResult {
    let original = standard_scope()?;
    for (family, literal, length) in [(3, DUE, 40), (4, FIFO, 48), (5, HISTORY, 38)] {
        let bytes = hex(literal)?;
        assert_eq!(bytes.len(), length);
        for end in 0..length {
            assert!(decode_for(family, &bytes[..end], &original, length, 134).is_err());
        }
        assert_eq!(
            decode_for(family, &bytes, &original, length - 1, 134).err(),
            Some(StateValueError::Limit {
                bytes: length,
                maximum: length - 1
            })
        );
        assert_eq!(
            decode_for(family, &bytes, &original, length, 133).err(),
            Some(StateValueError::Limit {
                bytes: 134,
                maximum: 133
            })
        );
        assert_eq!(
            decode_for(family, &bytes, &original, 0, 134).err(),
            Some(StateValueError::InvalidValue)
        );
        assert_eq!(
            decode_for(family, &bytes, &original, length, 0).err(),
            Some(StateValueError::InvalidValue)
        );
        let mut trailing = bytes.clone();
        trailing.push(1);
        assert_eq!(
            decode_for(family, &trailing, &original, length + 1, 134).err(),
            Some(StateValueError::Malformed)
        );
        for (offset, value, expected) in [
            (0, b'X', StateValueError::UnsupportedTag),
            (5, 2, StateValueError::UnsupportedVersion),
            (6, 2, StateValueError::UnsupportedTag),
            (9, 1, StateValueError::UnsupportedTag),
            (7, b'h', StateValueError::InvalidValue),
        ] {
            let mut bad = bytes.clone();
            bad[offset] = value;
            assert_eq!(
                decode_for(family, &bad, &original, length, 134).err(),
                Some(expected)
            );
        }
        for foreign in [
            scope(b"h", b"z", b"t")?,
            scope(b"g", b"y", b"t")?,
            scope(b"g", b"z", b"other")?,
        ] {
            assert_eq!(
                decode_for(family, &bytes, &foreign, length, 134).err(),
                Some(StateValueError::InvalidValue)
            );
        }
    }
    let mut bad = hex(DUE)?;
    bad[16] = 3;
    assert_eq!(
        decode_for(3, &bad, &original, 40, 134).err(),
        Some(StateValueError::InvalidValue)
    );
    let mut bad = hex(FIFO)?;
    bad[25] = 2;
    assert_eq!(
        decode_for(4, &bad, &original, 48, 134).err(),
        Some(StateValueError::UnsupportedTag)
    );
    let mut bad = hex(HISTORY)?;
    bad[29] = 101;
    assert_eq!(
        decode_for(5, &bad, &original, 38, 134).err(),
        Some(StateValueError::InvalidValue)
    );
    let mut bad = hex(HISTORY)?;
    bad[30] = b'-';
    assert_eq!(
        decode_for(5, &bad, &original, 38, 134).err(),
        Some(StateValueError::InvalidValue)
    );
    Ok(())
}

fn assert_selector_binding() -> TestResult {
    let scope = standard_scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    for wrong in [
        LaneKey {
            workflow_type: b"other",
            ..lane()
        },
        LaneKey {
            state: b"other",
            ..lane()
        },
        LaneKey {
            partition_key: b"other",
            ..lane()
        },
    ] {
        assert_eq!(
            decode_fifo_key(&hex(FIFO)?, &scope, wrong, 48, 134).err(),
            Some(StateValueError::InvalidValue)
        );
    }
    let wrong_partition = PartitionId::new(b"other".to_vec())?;
    let wrong_id = RecordId::new(b"other".to_vec())?;
    assert_eq!(
        decode_history_key(&hex(HISTORY)?, &scope, &wrong_partition, &id, 38, 134).err(),
        Some(StateValueError::InvalidValue)
    );
    assert_eq!(
        decode_history_key(&hex(HISTORY)?, &scope, &partition, &wrong_id, 38, 134).err(),
        Some(StateValueError::InvalidValue)
    );
    Ok(())
}

fn assert_encoder_refusals() -> TestResult {
    let scope = standard_scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let running = record(b"i\0\xff", true)?;
    let member = LaneOrder::for_record(&entry(&running, (1_u128 << 64) + 3));
    assert_eq!(
        encode_due_key(&scope, due(), 39),
        Err(StateValueError::Limit {
            bytes: 40,
            maximum: 39
        })
    );
    assert_eq!(
        encode_fifo_key(&scope, lane(), member, 47),
        Err(StateValueError::Limit {
            bytes: 48,
            maximum: 47
        })
    );
    assert_eq!(
        encode_history_key(&scope, &partition, &id, &history("100-10", 10), 37),
        Err(StateValueError::Limit {
            bytes: 38,
            maximum: 37
        })
    );
    assert_eq!(
        encode_due_prefix(&scope, 15),
        Err(StateValueError::Limit {
            bytes: 16,
            maximum: 15
        })
    );
    assert_eq!(
        encode_fifo_prefix(&scope, lane(), 24),
        Err(StateValueError::Limit {
            bytes: 25,
            maximum: 24
        })
    );
    assert_eq!(
        encode_history_prefix(&scope, &partition, &id, 21),
        Err(StateValueError::Limit {
            bytes: 22,
            maximum: 21
        })
    );
    assert_eq!(
        encode_due_key(
            &scope,
            DueOrder {
                priority: 3,
                ..due()
            },
            100
        ),
        Err(StateValueError::InvalidValue)
    );
    for event_id in [
        "01-1",
        "1-01",
        "-1-2",
        "1--2",
        "1",
        "1-2-x",
        "18446744073709551616-1",
        "1-18446744073709551616",
    ] {
        assert_eq!(
            encode_history_key(&scope, &partition, &id, &history(event_id, 1), 100),
            Err(StateValueError::InvalidValue)
        );
    }
    assert_eq!(
        encode_history_key(&scope, &partition, &id, &history("100-10", 11), 100),
        Err(StateValueError::InvalidValue)
    );
    assert_zero_caps(&scope, &partition, &id, member);
    Ok(())
}

fn assert_zero_caps(
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    member: LaneOrder<'_>,
) {
    assert_eq!(
        encode_due_key(scope, due(), 0),
        Err(StateValueError::InvalidValue)
    );
    assert_eq!(
        encode_fifo_key(scope, lane(), member, 0),
        Err(StateValueError::InvalidValue)
    );
    assert_eq!(
        encode_history_key(scope, partition, id, &history("0-0", 0), 0),
        Err(StateValueError::InvalidValue)
    );
    assert_eq!(
        encode_due_prefix(scope, 0),
        Err(StateValueError::InvalidValue)
    );
    assert_eq!(
        encode_fifo_prefix(scope, lane(), 0),
        Err(StateValueError::InvalidValue)
    );
    assert_eq!(
        encode_history_prefix(scope, partition, id, 0),
        Err(StateValueError::InvalidValue)
    );
}

fn assert_large_history_namespace() -> TestResult {
    let scope = scope(&[0; 256], &[0; 256], &[0; 8_192])?;
    let partition = PartitionId::new(vec![0; 65_535])?;
    let id = RecordId::new(vec![0; 65_535])?;
    let event = history("18446744073709551615-18446744073709551615", u64::MAX);
    let bytes = encode_history_key(&scope, &partition, &id, &event, 279_616)?;
    assert_eq!(bytes.len(), 279_616);
    let owner: DecodedOrderedKey =
        decode_history_key(&bytes, &scope, &partition, &id, 279_616, 169)?;
    assert_eq!(owner.logical_bytes(), 169);
    assert_eq!(
        owner.history_position(),
        Some((u64::MAX, event.event_id.as_str()))
    );
    assert_eq!(
        decode_history_key(&bytes, &scope, &partition, &id, 279_616, 168).err(),
        Some(StateValueError::Limit {
            bytes: 169,
            maximum: 168
        })
    );
    Ok(())
}

#[test]
fn ordered_key_decoding_caps_and_corruption_preserve_owner() -> TestResult {
    assert_decoder_refusals()?;
    assert_selector_binding()?;
    assert_encoder_refusals()?;
    assert_large_history_namespace()
}
