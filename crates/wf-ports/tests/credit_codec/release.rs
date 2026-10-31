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
    assert_eq!(encode_release_intent(intent, 65_535)?, expected);
    let view = inspect_release_intent(&expected, 65_535)?;
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
        65_535,
    )?;
    assert_eq!(
        inspect_release_intent(&empty, 65_535)?.partition,
        Some(b"".as_slice())
    );
    assert_ne!(empty, expected);
    for end in 0..expected.len() {
        assert!(inspect_release_intent(&expected[..end], 65_535).is_err());
    }
    let mut trailing = expected.clone();
    trailing.push(0);
    assert!(inspect_release_intent(&trailing, 65_535).is_err());
    assert_eq!(
        inspect_release_intent(&expected, expected.len() - 1).err(),
        Some(E::ByteLimit)
    );
    assert!(inspect_release_intent(&[131, 80, 0, 0, 0, 1, 0], 65_535).is_err());
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

#[test]
fn release_binding_matches_independent_frame_and_borrows_complete_original_intent() -> TestResult {
    use wf_ports::log::{ReleaseBindingRef, encode_release_binding, inspect_release_binding};
    let intent = ReleaseIntentRef {
        flow_id: b"flow\0id",
        partition: Some(b"p\0"),
        scope: b"scope",
        shard_id: 3,
        reservation_id: b"paid\0id",
        enforcement: LimitEnforcement::ApproximateGlobal,
        created_at: TimestampMillis::new(100),
    };
    let mut expected = b"FWGB\0\x01".to_vec();
    expected.extend_from_slice(&7_u64.to_be_bytes());
    expected.extend_from_slice(&encode_release_intent(intent, 65536)?);
    assert_eq!(
        encode_release_binding(ReleaseBindingRef { epoch: 7, intent }, expected.len())?,
        expected
    );
    let view = inspect_release_binding(&expected, expected.len())?;
    assert_eq!(view.epoch, 7);
    assert_eq!(view.intent.flow_id, b"flow\0id");
    assert_eq!(view.intent.partition, Some(b"p\0".as_slice()));
    assert_eq!(view.intent.reservation_id, b"paid\0id");
    assert!(view.intent.enforcement == LimitEnforcement::ApproximateGlobal);
    assert_eq!(view.intent.flow_id.as_ptr(), expected[24..].as_ptr());
    for end in 0..expected.len() {
        assert!(inspect_release_binding(&expected[..end], expected.len()).is_err());
    }
    let mut trailing = expected.clone();
    trailing.push(0);
    assert!(inspect_release_binding(&trailing, trailing.len()).is_err());
    assert_eq!(
        inspect_release_binding(&expected, expected.len() - 1).err(),
        Some(E::ByteLimit)
    );
    assert_eq!(
        encode_release_binding(ReleaseBindingRef { epoch: 7, intent }, expected.len() - 1).err(),
        Some(E::ByteLimit)
    );
    for epoch in [0, wf_domain::workflow::MAX_EXACT_MILLIS + 1, u64::MAX] {
        let mut bad = expected.clone();
        bad[6..14].copy_from_slice(&epoch.to_be_bytes());
        assert!(inspect_release_binding(&bad, bad.len()).is_err());
        assert!(encode_release_binding(ReleaseBindingRef { epoch, intent }, 65536).is_err());
    }
    Ok(())
}

#[test]
fn release_backlog_rejection_is_whole_command_and_closed_to_selected_retained_profile() -> TestResult
{
    use wf_domain::workflow::{
        admission::AdmissionLimits,
        indexed::IndexRejection,
        request::{ClientIntent, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus},
    };
    use wf_ports::log::{
        public_mutation_retained_status, record_batch_retained_summary,
        workflow_value_retained_summary,
    };
    use wf_types::{
        identity::{GroupId, RecoveryId, RequestId, TenantId},
        version::{AppliedPosition, LogIdentity},
    };
    let admission = AdmissionLimits::default();
    let intent = ClientIntent::workflow_stored_value_batch([7; 32]);
    let log = LogIdentity::new(1, 7, AppliedPosition::new(0));
    let time = TimestampMillis::new(100);
    let status = RetainedStatus::IndexedRejected(IndexRejection::ReleaseBacklogFull);
    for version in [5, 6, 8] {
        assert_eq!(
            workflow_value_retained_summary(intent, version, &[1, 8, 0], log, time, admission)?,
            (status, None)
        );
        for malformed in [b"".as_slice(), &[1], &[1, 8], &[1, 8, 1], &[1, 8, 0, 0]] {
            assert!(
                workflow_value_retained_summary(intent, version, malformed, log, time, admission)
                    .is_err()
            );
        }
        assert!(
            workflow_value_retained_summary(
                ClientIntent::workflow_value_batch([7; 32]),
                version,
                &[1, 8, 0],
                log,
                time,
                admission
            )
            .is_err()
        );
    }
    assert!(
        workflow_value_retained_summary(intent, 6, &[2, 0, 0, 0, 1, 1, 8, 0], log, time, admission)
            .is_err()
    );
    assert!(workflow_value_retained_summary(intent, 8, &[2, 8, 0], log, time, admission).is_err());
    for intent in [
        ClientIntent::record_batch([7; 32]),
        ClientIntent::independent_record_batch([7; 32]),
    ] {
        assert!(record_batch_retained_summary(intent, 1, &[1, 8, 0]).is_err());
    }
    assert!(
        public_mutation_retained_status(ClientIntent::new(1, [7; 32])?, 2, &[1, 8, 0]).is_err()
    );
    for (intent, response) in [
        (intent, 5),
        (intent, 6),
        (intent, 8),
        (intent, 7),
        (ClientIntent::workflow_value_batch([7; 32]), 5),
        (ClientIntent::record_batch([7; 32]), 1),
        (ClientIntent::new(1, [7; 32])?, 2),
    ] {
        let metadata = RetainedMetadata {
            scope: RequestScope::new(
                TenantId::new(b"t".to_vec())?,
                GroupId::new(b"g".to_vec())?,
                RecoveryId::new(b"r".to_vec())?,
            ),
            request: RequestId::new(b"q".to_vec())?,
            intent,
            original_position: log,
            original_time: time,
            expires_at: TimestampMillis::new(200),
            status,
        };
        assert_eq!(
            RetainedOutcome::new(metadata, response, vec![1, 8, 0], admission).is_ok(),
            intent.version() == 9 && matches!(response, 5 | 6 | 8)
        );
    }
    Ok(())
}
