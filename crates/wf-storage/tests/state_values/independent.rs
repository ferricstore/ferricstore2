//! Frozen V6 response bytes and separate wire, owner and record-profile caps.

use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::indexed::IndexRejection;
use wf_domain::workflow::request::{
    ClientIntent, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
};
use wf_ports::log::record_batch_retained_summary;
use wf_storage::state::value::{
    StateValueError, decode_outcome, encode_outcome, scan_outcome, scan_record_point,
    scan_record_point_profiled,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, LogIdentity, TimestampMillis};

use super::TestResult;

// Independent FWSO1 header: t/g/z/q, intent6/digest11, log3/4/0,
// original5/expiry105, response1. RAW response length begins at byte102.
const HEADER: &str = "4657534f000100000001740000000167000000017a000000017100061111111111111111111111111111111111111111111111111111111111111111000000000000000300000000000000040000000000000000000000000000000500000000000000690001";
// Four rows: success, NotFound, FifoPriority, CounterOverflow(Fence).
// U16 + identity payload4 + wire fixed102 =122; owner512+4+16=532.
const MIXED: &str = "4657534f000100000001740000000167000000017a0000000171000611111111111111111111111111111111111111111111111111111111111111110000000000000003000000000000000400000000000000000000000000000005000000000000006900010000001002000000040001000001050001001001";
// Frozen created record: seven binary names of length3, due7, present-empty
// payload, priority2, sequence u128::MAX-7. Core9; owner512+3+3+9=527.
const RECORD: &str = "465753520001000000036700ff000000037a00ff000000037400ff000000037000ff02fffffffffffffffffffffffffffffff8000000036900ff000000037200ff000000037100ff000000000000000100000000000000000000000000000000000000000000000100000000000000010100000000000000070000010000000000000000000000000000010000000000000000";

fn hex(value: &str) -> TestResult<Vec<u8>> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn scope(tenant: &[u8], group: &[u8], recovery: &[u8]) -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(tenant.to_vec())?,
        GroupId::new(group.to_vec())?,
        RecoveryId::new(recovery.to_vec())?,
    ))
}

fn query() -> TestResult<(RequestScope, RequestId)> {
    Ok((scope(b"t", b"g", b"z")?, RequestId::new(b"q".to_vec())?))
}

fn frame(response: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = hex(HEADER)?;
    bytes.extend_from_slice(&u32::try_from(response.len())?.to_be_bytes());
    bytes.extend_from_slice(response);
    Ok(bytes)
}

fn retained(
    response: Vec<u8>,
    status: RetainedStatus,
    limits: AdmissionLimits,
) -> TestResult<RetainedOutcome> {
    let (scope, request) = query()?;
    Ok(RetainedOutcome::new(
        RetainedMetadata {
            scope,
            request,
            intent: ClientIntent::independent_record_batch([0x11; 32]),
            original_position: LogIdentity::new(3, 4, AppliedPosition::new(0)),
            original_time: TimestampMillis::new(5),
            expires_at: TimestampMillis::new(105),
            status,
        },
        1,
        response,
        limits,
    )?)
}

fn assert_row(row: &[u8]) -> TestResult {
    let mut body = vec![2, 0, 0, 0, 1];
    body.extend_from_slice(row);
    assert_eq!(
        record_batch_retained_summary(
            ClientIntent::independent_record_batch([0x11; 32]),
            1,
            &body
        )?,
        (RetainedStatus::Accepted, Some(1))
    );
    let (expected, request) = query()?;
    let bytes = frame(&body)?;
    let scanned = scan_outcome(
        &bytes,
        &expected,
        &request,
        AdmissionLimits::default(),
        bytes.len(),
    )?;
    assert_eq!(scanned.response_count(), Some(1));
    assert_eq!(scanned.into_owned(1_024)?.response(), body);
    Ok(())
}

fn accepted_rows() -> TestResult {
    assert_row(&[0])?;
    assert_row(&[1, 4, 0])?;
    assert_row(&[1, 5, 0])?;
    for code in 0..=22 {
        if !matches!(code, 8 | 16 | 19) {
            assert_row(&[1, 0, code])?;
        }
    }
    for (code, count) in [(8, 7), (16, 3), (19, 8)] {
        for detail in 0..count {
            assert_row(&[1, 0, code, detail])?;
        }
    }
    Ok(())
}

fn rejected_rows() -> TestResult {
    let intent = ClientIntent::independent_record_batch([0x11; 32]);
    let (expected, request) = query()?;
    for row in [
        &[1, 1, 0][..], // Admission is whole-call only.
        &[1, 2, 0],     // DuplicateId.
        &[1, 3, 0],     // CrossGroup.
        &[1, 6, 0],     // NotFifoHead.
        &[1, 4, 1],
        &[1, 5, 1],
        &[1, 0, 23],
        &[1, 0, 8, 7],
        &[1, 0, 16, 3],
        &[1, 0, 19, 8],
        &[3],
    ] {
        let mut body = vec![2, 0, 0, 0, 1];
        body.extend_from_slice(row);
        assert!(record_batch_retained_summary(intent, 1, &body).is_err());
        let bytes = frame(&body)?;
        assert!(
            scan_outcome(
                &bytes,
                &expected,
                &request,
                AdmissionLimits::default(),
                1_024
            )
            .is_err()
        );
    }
    for body in [
        &[][..],
        &[0],
        &[2],
        &[2, 0, 0, 0],
        &[2, 0, 0, 0, 0],
        &[2, 0, 0, 0, 1],
        &[2, 0, 0, 0, 2, 0],
        &[2, 0, 0, 0, 1, 0, 0],
        &[2, 0, 1, 0x86, 0xa1], // Count100001, beyond the grammar ceiling.
        &[3, 0, 0, 0, 1, 0],
    ] {
        assert!(record_batch_retained_summary(intent, 1, body).is_err());
        assert!(
            scan_outcome(
                &frame(body)?,
                &expected,
                &request,
                AdmissionLimits::default(),
                1_024
            )
            .is_err()
        );
    }
    let mut maximum = vec![2, 0, 1, 0x86, 0xa0];
    maximum.resize(100_005, 0);
    assert_eq!(
        record_batch_retained_summary(intent, 1, &maximum)?,
        (RetainedStatus::Accepted, Some(100_000))
    );
    assert!(
        record_batch_retained_summary(ClientIntent::record_batch([0x11; 32]), 1, &maximum).is_err()
    );
    assert!(record_batch_retained_summary(intent, 2, &maximum).is_err());
    Ok(())
}

fn outcome_corruption() -> TestResult {
    let bytes = hex(MIXED)?;
    let (expected, request) = query()?;
    let limits = AdmissionLimits::default();
    for end in 0..bytes.len() {
        assert!(scan_outcome(&bytes[..end], &expected, &request, limits, 122).is_err());
    }
    for (offset, value, error) in [
        (0, b'X', StateValueError::UnsupportedTag),
        (5, 2, StateValueError::UnsupportedVersion),
        (27, 7, StateValueError::UnsupportedVersion),
        (101, 2, StateValueError::UnsupportedVersion),
    ] {
        let mut bad = bytes.clone();
        bad[offset] = value;
        assert!(
            matches!(scan_outcome(&bad, &expected, &request, limits, 122), Err(actual) if actual == error)
        );
    }
    let mut expired = bytes.clone();
    expired[92..100].copy_from_slice(&5_u64.to_be_bytes());
    assert!(matches!(
        scan_outcome(&expired, &expected, &request, limits, 122),
        Err(StateValueError::InvalidValue)
    ));
    for field in 0..4 {
        let foreign = scope(
            if field == 0 { b"other" } else { b"t" },
            if field == 1 { b"other" } else { b"g" },
            if field == 2 { b"other" } else { b"z" },
        )?;
        let query = RequestId::new(if field == 3 {
            b"other".to_vec()
        } else {
            b"q".to_vec()
        })?;
        assert!(matches!(
            scan_outcome(&bytes, &foreign, &query, limits, 122),
            Err(StateValueError::InvalidValue)
        ));
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    let foreign = scope(b"other", b"g", b"z")?;
    for expected in [&expected, &foreign] {
        assert!(matches!(
            scan_outcome(&trailing, expected, &request, limits, 123),
            Err(StateValueError::Malformed)
        ));
    }
    let mut length = bytes;
    length[102..106].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(
        scan_outcome(&length, &expected, &request, limits, 122),
        Err(StateValueError::Malformed)
    ));
    Ok(())
}

fn rejected_outcome() -> TestResult {
    let limits = AdmissionLimits::default();
    let status =
        RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::DuplicateId));
    let body = vec![1, 2, 0];
    let outcome = retained(body.clone(), status, limits)?;
    assert_eq!(encode_outcome(&outcome, 109)?, frame(&body)?);
    let bytes = frame(&body)?;
    let (expected, request) = query()?;
    let scanned = scan_outcome(&bytes, &expected, &request, limits, 109)?;
    assert_eq!(scanned.response_count(), None);
    assert_eq!(scanned.owned_bytes(), 519);
    assert_eq!(scanned.into_owned(519)?, outcome);
    for (body, status) in [
        (
            hex(MIXED)?[106..].to_vec(),
            RetainedStatus::IndexedRejected(IndexRejection::InvalidPriority),
        ),
        (body, RetainedStatus::Accepted),
    ] {
        let mismatch = retained(body, status, limits)?;
        assert_eq!(
            encode_outcome(&mismatch, 1_024),
            Err(StateValueError::InvalidValue)
        );
    }
    Ok(())
}

fn large_outcome() -> TestResult {
    // Independent source operands: [02,u32BE5500] + 5500 NotFound rows.
    let mut body = vec![2, 0, 0, 0x15, 0x7c];
    for _ in 0..5_500 {
        body.extend_from_slice(&[1, 0, 0]);
    }
    assert_eq!(body.len(), 16_505);
    let bytes = frame(&body)?;
    assert_eq!(bytes.len(), 16_611);
    let limits = AdmissionLimits::new(256, 16 * 1_024 * 1_024, 5_500)?;
    let expected = retained(body, RetainedStatus::Accepted, limits)?;
    assert_eq!(encode_outcome(&expected, 16_611)?, bytes);
    assert_eq!(
        encode_outcome(&expected, 16_610),
        Err(StateValueError::Limit {
            bytes: 16_611,
            maximum: 16_610
        })
    );
    let (scope, request) = query()?;
    let scanned = scan_outcome(&bytes, &scope, &request, limits, 16_611)?;
    assert_eq!(scanned.response_count(), Some(5_500));
    assert_eq!(scanned.owned_bytes(), 17_021);
    assert_eq!(
        scanned.into_owned(17_020),
        Err(StateValueError::Limit {
            bytes: 17_021,
            maximum: 17_020
        })
    );
    assert!(matches!(
        scan_outcome(&bytes, &scope, &request, limits, 16_610),
        Err(StateValueError::Limit {
            bytes: 16_611,
            maximum: 16_610
        })
    ));
    // Structural scanning enforces100000. The backend separately checks the
    // captured batch_items before R/copy; this view grants no such certificate.
    let structural = AdmissionLimits::new(256, 16 * 1_024 * 1_024, 1)?;
    assert_eq!(
        scan_outcome(&bytes, &scope, &request, structural, 16_611)?.response_count(),
        Some(5_500)
    );
    let scanned = scan_outcome(&bytes, &scope, &request, limits, 16_611)?;
    drop(scope);
    drop(request);
    let owner = scanned.into_owned(17_021)?;
    drop(bytes);
    assert_eq!(owner, expected);
    Ok(())
}

#[test]
fn v6_response_owner_roundtrip() -> TestResult {
    let bytes = hex(MIXED)?;
    assert_eq!(bytes.len(), 122);
    assert_eq!(
        frame(&[2, 0, 0, 0, 4, 0, 1, 0, 0, 1, 5, 0, 1, 0, 16, 1])?,
        bytes
    );
    let (scope, request) = query()?;
    let limits = AdmissionLimits::new(1, 16, 4)?;
    for maximum in [0, 531] {
        let scanned = scan_outcome(&bytes, &scope, &request, limits, 122)?;
        assert_eq!(scanned.response_count(), Some(4));
        assert_eq!(scanned.owned_bytes(), 532);
        let error = if maximum == 0 {
            StateValueError::InvalidValue
        } else {
            StateValueError::Limit {
                bytes: 532,
                maximum,
            }
        };
        assert_eq!(scanned.into_owned(maximum), Err(error));
    }
    assert!(matches!(
        scan_outcome(&bytes, &scope, &request, limits, 121),
        Err(StateValueError::Limit {
            bytes: 122,
            maximum: 121
        })
    ));
    assert!(matches!(
        scan_outcome(&bytes, &scope, &request, limits, 0),
        Err(StateValueError::InvalidValue)
    ));
    let scanned = scan_outcome(&bytes, &scope, &request, limits, 122)?;
    drop(scope);
    drop(request);
    let owner = scanned.into_owned(532)?;
    assert_eq!(encode_outcome(&owner, 122)?, bytes);
    assert_eq!(
        encode_outcome(&owner, 121),
        Err(StateValueError::Limit {
            bytes: 122,
            maximum: 121
        })
    );
    let (scope, request) = query()?;
    assert_eq!(
        decode_outcome(&bytes, &scope, &request, limits, 122)?,
        owner
    );
    drop(bytes);
    assert_eq!(owner.scope(), &scope);
    assert_eq!(owner.request(), &request);
    assert_eq!(
        owner.intent(),
        ClientIntent::independent_record_batch([0x11; 32])
    );
    assert_eq!(
        owner.original_position(),
        LogIdentity::new(3, 4, AppliedPosition::new(0))
    );
    assert_eq!(owner.original_time(), TimestampMillis::new(5));
    assert_eq!(owner.expires_at(), TimestampMillis::new(105));
    assert_eq!(owner.response_version(), 1);
    assert_eq!(owner.status(), RetainedStatus::Accepted);
    assert_eq!(
        owner.response(),
        [2, 0, 0, 0, 4, 0, 1, 0, 0, 1, 5, 0, 1, 0, 16, 1]
    );
    accepted_rows()?;
    rejected_rows()?;
    outcome_corruption()?;
    rejected_outcome()?;
    large_outcome()
}

fn record_corruption() -> TestResult {
    let bytes = hex(RECORD)?;
    let expected = scope(b"t\0\xff", b"g\0\xff", b"z\0\xff")?;
    let partition = PartitionId::new(b"p\0\xff".to_vec())?;
    let id = RecordId::new(b"i\0\xff".to_vec())?;
    let limits = AdmissionLimits::new(3, 9, 1)?;
    for end in 0..bytes.len() {
        assert!(
            scan_record_point_profiled(&bytes[..end], &expected, &partition, &id, limits, 147)
                .is_err()
        );
    }
    for (offset, value, error) in [
        (0, b'X', StateValueError::UnsupportedTag),
        (5, 2, StateValueError::UnsupportedVersion),
        (34, 3, StateValueError::InvalidValue),
        (112, 2, StateValueError::UnsupportedTag),
        (146, 1, StateValueError::InvalidValue),
    ] {
        let mut bad = bytes.clone();
        bad[offset] = value;
        for result in [
            scan_record_point(&bad, &expected, &partition, &id, limits, 147),
            scan_record_point_profiled(&bad, &expected, &partition, &id, limits, 147),
        ] {
            assert!(matches!(result, Err(actual) if actual == error));
        }
    }
    for field in 0..5 {
        let foreign = scope(
            if field == 0 { b"other" } else { b"t\0\xff" },
            if field == 1 { b"other" } else { b"g\0\xff" },
            if field == 2 { b"other" } else { b"z\0\xff" },
        )?;
        let partition = PartitionId::new(if field == 3 {
            b"other".to_vec()
        } else {
            b"p\0\xff".to_vec()
        })?;
        let id = RecordId::new(if field == 4 {
            b"other".to_vec()
        } else {
            b"i\0\xff".to_vec()
        })?;
        assert!(matches!(
            scan_record_point_profiled(&bytes, &foreign, &partition, &id, limits, 147),
            Err(StateValueError::InvalidValue)
        ));
    }
    let mut trailing = bytes;
    trailing.push(0);
    let foreign = scope(b"other", b"g\0\xff", b"z\0\xff")?;
    for expected in [&expected, &foreign] {
        assert!(matches!(
            scan_record_point_profiled(&trailing, expected, &partition, &id, limits, 148),
            Err(StateValueError::Malformed)
        ));
    }
    Ok(())
}

#[test]
fn profiled_record_limit_vs_corruption() -> TestResult {
    let bytes = hex(RECORD)?;
    assert_eq!(bytes.len(), 147);
    let expected = scope(b"t\0\xff", b"g\0\xff", b"z\0\xff")?;
    let partition = PartitionId::new(b"p\0\xff".to_vec())?;
    let id = RecordId::new(b"i\0\xff".to_vec())?;
    let limits = AdmissionLimits::new(3, 9, 1)?;
    let legacy =
        scan_record_point(&bytes, &expected, &partition, &id, limits, 147)?.into_owned(527)?;
    for (limits, error) in [
        (
            AdmissionLimits::new(2, 9, 1)?,
            StateValueError::Limit {
                bytes: 3,
                maximum: 2,
            },
        ),
        (
            AdmissionLimits::new(3, 8, 1)?,
            StateValueError::Limit {
                bytes: 9,
                maximum: 8,
            },
        ),
    ] {
        assert!(matches!(
            scan_record_point(&bytes, &expected, &partition, &id, limits, 147),
            Err(StateValueError::InvalidValue)
        ));
        assert!(
            matches!(scan_record_point_profiled(&bytes, &expected, &partition, &id, limits, 147), Err(actual) if actual == error)
        );
    }
    assert!(matches!(
        scan_record_point_profiled(&bytes, &expected, &partition, &id, limits, 146),
        Err(StateValueError::Limit {
            bytes: 147,
            maximum: 146
        })
    ));
    assert!(matches!(
        scan_record_point_profiled(&bytes, &expected, &partition, &id, limits, 0),
        Err(StateValueError::InvalidValue)
    ));
    let scanned = scan_record_point_profiled(&bytes, &expected, &partition, &id, limits, 147)?;
    assert_eq!(scanned.owned_bytes(), 527);
    assert_eq!(scanned.version(), 1);
    assert_eq!(
        scanned.into_owned(526),
        Err(StateValueError::Limit {
            bytes: 527,
            maximum: 526
        })
    );
    let scanned = scan_record_point_profiled(&bytes, &expected, &partition, &id, limits, 147)?;
    drop(expected);
    drop(partition);
    drop(id);
    let owner = scanned.into_owned(527)?;
    drop(bytes);
    assert_eq!(owner, legacy);
    assert_eq!(owner.record.payload(), Some(b"".as_slice()));
    assert_eq!(owner.record.next_run_at(), Some(TimestampMillis::new(7)));
    assert_eq!(owner.ordering.priority, 2);
    assert_eq!(owner.ordering.state_enter_seq, u128::MAX - 7);
    record_corruption()
}
