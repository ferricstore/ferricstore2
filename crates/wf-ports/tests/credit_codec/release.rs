//! Independent complete native frames and pinned source cursor observations.
use super::TestResult;
use wf_domain::governance::release::{LimitEnforcement, ReleaseIntentRef, ReleaseOutboxState};
use wf_ports::log::{
    CodecError as E, ReleaseReconcileCursor, decode_release_cursor, encode_release_cursor,
    encode_release_intent, encode_release_meta, inspect_release_intent, inspect_release_meta,
};
use wf_types::version::TimestampMillis;
fn raw(out: &mut Vec<u8>, value: &[u8]) -> TestResult {
    out.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}
#[test]
fn release_meta_matches_independent_frame_and_complete_finite_validation() -> TestResult {
    let mut expected = b"FWRO\0\x01".to_vec();
    expected.extend_from_slice(&1_u64.to_be_bytes());
    expected.extend_from_slice(&0_u64.to_be_bytes());
    assert_eq!(
        encode_release_meta(ReleaseOutboxState { head: 1, tail: 0 }, 22)?,
        expected
    );
    let state = inspect_release_meta(&expected, 22)?;
    assert_eq!((state.head, state.tail), (1, 0));
    for end in 0..expected.len() {
        assert!(inspect_release_meta(&expected[..end], 22).is_err());
    }
    let mut trailing = expected.clone();
    trailing.push(0);
    assert!(inspect_release_meta(&trailing, 23).is_err());
    assert_eq!(
        inspect_release_meta(&expected, 21).err(),
        Some(E::ByteLimit)
    );
    expected[13] = 2;
    assert!(inspect_release_meta(&expected, 22).is_err());
    Ok(())
}
#[test]
fn release_intent_matches_independent_bytes_nil_empty_partition_and_borrowed_fields() -> TestResult
{
    let intent = ReleaseIntentRef {
        flow_id: b"flow",
        partition: None,
        scope: b"scope",
        shard_id: 7,
        reservation_id: b"\0\xff",
        enforcement: LimitEnforcement::StrictGlobal,
        created_at: TimestampMillis::new(1000),
    };
    let mut expected = b"FWRI\0\x01".to_vec();
    raw(&mut expected, b"flow")?;
    expected.push(0);
    raw(&mut expected, b"scope")?;
    expected.extend_from_slice(&7_u64.to_be_bytes());
    raw(&mut expected, b"\0\xff")?;
    expected.push(1);
    expected.extend_from_slice(&1000_u64.to_be_bytes());
    assert_eq!(encode_release_intent(intent, 65535)?, expected);
    let view = inspect_release_intent(&expected, 65535)?;
    assert_eq!(view.flow_id, b"flow");
    assert_eq!(view.partition, None);
    assert_eq!(view.scope, b"scope");
    assert_eq!(view.reservation_id, b"\0\xff");
    assert_eq!(view.shard_id, 7);
    assert_eq!(view.flow_id.as_ptr(), expected[10..].as_ptr());
    let empty = encode_release_intent(
        ReleaseIntentRef {
            partition: Some(b""),
            ..intent
        },
        65535,
    )?;
    assert_eq!(
        inspect_release_intent(&empty, 65535)?.partition,
        Some(b"".as_slice())
    );
    assert_ne!(empty, expected);
    for end in 0..expected.len() {
        assert!(inspect_release_intent(&expected[..end], 65535).is_err());
    }
    let mut trailing = expected.clone();
    trailing.push(0);
    assert!(inspect_release_intent(&trailing, 65535).is_err());
    assert_eq!(
        inspect_release_intent(&expected, expected.len() - 1).err(),
        Some(E::ByteLimit)
    );
    assert!(inspect_release_intent(&[131, 80, 0, 0, 0, 1, 0], 65535).is_err());
    Ok(())
}
#[test]
fn release_cursor_preserves_source_v1_sorted_output_and_accepted_integer_grammar() -> TestResult {
    let cursor = ReleaseReconcileCursor::from_parts(2, &[(1, 17), (0, 257)], 4, 160)?;
    assert_eq!(encode_release_cursor(&cursor, 512)?, b"v1:2|0=257,1=17");
    let original = decode_release_cursor(Some(b"v1:+02|+01=0017,-0=+0257"), 4, 160)?;
    assert_eq!(original.positions(), &[(0, 257), (1, 17)]);
    assert_eq!(original.next_shard(), 2);
    assert_eq!(encode_release_cursor(&original, 512)?, b"v1:2|0=257,1=17");
    let empty = decode_release_cursor(None, 4, 128)?;
    assert_eq!(empty.next_shard(), 0);
    assert_eq!(empty.positions().len(), 0);
    for bytes in [
        b"v1:2".as_slice(),
        b"v2:2|0=257",
        b"v1:2|0=1,+0=2",
        b"v1:4|",
        b"v1:0|0=0",
        b"v1:0|0=9007199254740992",
        b"v1:0|0=1 ",
        b"v1:0|0=-1",
    ] {
        assert!(decode_release_cursor(Some(bytes), 4, 4096).is_err());
    }
    Ok(())
}
#[test]
fn release_cursor_admits_wire_and_original_flat_owner_before_allocation() -> TestResult {
    assert_eq!(
        decode_release_cursor(Some(b"v1:2|0=257,1=17"), 4, 159).err(),
        Some(E::OwnedLimit)
    );
    assert_eq!(
        decode_release_cursor(None, 4, 127).err(),
        Some(E::OwnedLimit)
    );
    assert!(decode_release_cursor(None, 0, 128).is_err());
    let empty = decode_release_cursor(None, u64::MAX, 128)?;
    assert_eq!(empty.shard_count(), u64::MAX);
    assert_eq!(encode_release_cursor(&empty, 4).err(), Some(E::ByteLimit));
    assert_eq!(
        decode_release_cursor(Some(&[b'0'; 513]), 4, 4096).err(),
        Some(E::ByteLimit)
    );
    let exact = decode_release_cursor(Some(b"v1:2|0=257,1=17"), 4, 160)?;
    assert_eq!(encode_release_cursor(&exact, 14).err(), Some(E::ByteLimit));
    Ok(())
}
