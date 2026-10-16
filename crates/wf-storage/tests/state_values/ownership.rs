//! Exact point binding and prospective owned-field admission from frozen wire bytes.

use wf_domain::history::EventKind;
use wf_domain::workflow::MAX_EXACT_MILLIS;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedStatus};
use wf_ports::state::StoredWorkflowRecord;
use wf_storage::state::value::{
    StateValueError, decode_outcome, decode_record, encode_record_fields, scan_outcome,
    scan_record_point,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, Fence, LogIdentity, TimestampMillis};

use super::TestResult;

// Independent protocol pack: wire269; all eleven owned variable lengths sum113.
// 2+3+4+5+7+9+11+13+17+19+23 + fixed512 =625, distinct from encoded length.
const RUNNING: &str = "4657535200010000000167000000017a0000000274ff000000037000ff02fffffffffffffffffffffffffffffff8000000046900ff780000000574797065ff0000000772756e6e696e670000000000000007000000000000000900000000000000020000000000000001000000000000000a0100000000000000140100000009737461746500ff7879010000000b776f726b657200ff78797a0000000d746f6b656e00ff6162636465660000000000000014010000001100616161616161616161616161616161ff0100000013006262626262626262626262626262626262ff010000001700636363636363636363636363636363636363636363ff0100000000000000070000000000000009";
// Independent FWSO1: copied identity lengths2+3+4+5 and response1 =>527.
const OUTCOME: &str = "4657534f00010000000274ff000000036700ff000000047a00ff78000000057100ff7879000511111111111111111111111111111111111111111111111111111111111111110000000000000003000000000000000400000000000000000000000000000005000000000000006900010000000100";
const EMPTY_PAYLOAD: &str = "465753520001000000036700ff000000037a00ff000000037400ff000000037000ff02fffffffffffffffffffffffffffffff8000000036900ff000000037200ff000000037100ff000000000000000100000000000000000000000000000000000000000000000100000000000000010100000000000000070000010000000000000000000000000000010000000000000000";

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

fn record_query() -> TestResult<(RequestScope, PartitionId, RecordId)> {
    Ok((
        scope(b"t\xff", b"g", b"z")?,
        PartitionId::new(b"p\0\xff".to_vec())?,
        RecordId::new(b"i\0\xffx".to_vec())?,
    ))
}

fn outcome_query() -> TestResult<(RequestScope, RequestId)> {
    Ok((
        scope(b"t\xff", b"g\0\xff", b"z\0\xffx")?,
        RequestId::new(b"q\0\xffxy".to_vec())?,
    ))
}

fn assert_running(owner: &StoredWorkflowRecord) -> TestResult {
    assert_eq!(owner.tenant.as_bytes(), b"t\xff");
    assert_eq!(owner.partition.as_bytes(), b"p\0\xff");
    assert_eq!(owner.ordering.priority, 2);
    assert_eq!(owner.ordering.state_enter_seq, u128::MAX - 7);
    let record = &owner.record;
    assert_eq!(record.id().as_bytes(), b"i\0\xffx");
    assert_eq!(record.workflow_type(), b"type\xff");
    assert_eq!(record.state(), b"running");
    assert_eq!(record.version(), 7);
    assert_eq!(record.fence(), Fence::new(9));
    assert_eq!(record.attempts(), 2);
    assert_eq!(record.created_at(), TimestampMillis::new(1));
    assert_eq!(record.updated_at(), TimestampMillis::new(10));
    assert_eq!(record.next_run_at(), Some(TimestampMillis::new(20)));
    assert_eq!(record.run_state(), Some(b"state\0\xffxy".as_slice()));
    let lease = record.lease().ok_or("running lease absent")?;
    assert_eq!(lease.worker, b"worker\0\xffxyz");
    assert_eq!(lease.token.as_bytes(), b"token\0\xffabcdef");
    assert_eq!(lease.deadline, TimestampMillis::new(20));
    assert_eq!(record.payload(), Some(b"\0aaaaaaaaaaaaaaa\xff".as_slice()));
    assert_eq!(record.result(), Some(b"\0bbbbbbbbbbbbbbbbb\xff".as_slice()));
    assert_eq!(
        record.error(),
        Some(b"\0ccccccccccccccccccccc\xff".as_slice())
    );
    assert_eq!(
        record.latest_event_witness(),
        (EventKind::Claimed, 7, Fence::new(9))
    );
    Ok(())
}

#[test]
fn record_point_scan_binds_every_expected_component() -> TestResult {
    let bytes = hex(RUNNING)?;
    assert_eq!(bytes.len(), 269);
    let (expected, partition, id) = record_query()?;
    let scanned = scan_record_point(
        &bytes,
        &expected,
        &partition,
        &id,
        AdmissionLimits::default(),
        269,
    )?;
    // Query owners do not belong to the returned scan's lifetime.
    drop(expected);
    drop(partition);
    drop(id);
    assert_eq!(scanned.owned_bytes(), 625);
    assert_eq!(scanned.version(), 7);
    assert_running(&scanned.into_owned(625)?)?;
    for field in 0..5 {
        let expected = scope(
            if field == 0 { b"other" } else { b"t\xff" },
            if field == 1 { b"other" } else { b"g" },
            if field == 2 { b"other" } else { b"z" },
        )?;
        let partition = PartitionId::new(if field == 3 {
            b"other".to_vec()
        } else {
            b"p\0\xff".to_vec()
        })?;
        let id = RecordId::new(if field == 4 {
            b"other".to_vec()
        } else {
            b"i\0\xffx".to_vec()
        })?;
        assert!(matches!(
            scan_record_point(
                &bytes,
                &expected,
                &partition,
                &id,
                AdmissionLimits::default(),
                269,
            ),
            Err(StateValueError::InvalidValue)
        ));
    }
    Ok(())
}

#[test]
fn record_scan_admits_all_owned_fields_before_copy() -> TestResult {
    let bytes = hex(RUNNING)?;
    let (expected, partition, id) = record_query()?;
    // Core domain fields total108; tenant2/partition3 and fixed512 are separate.
    let limits = AdmissionLimits::new(23, 108, 1)?;
    for maximum in [0, 624] {
        let scanned = scan_record_point(&bytes, &expected, &partition, &id, limits, 269)?;
        assert_eq!(scanned.owned_bytes(), 625);
        let error = if maximum == 0 {
            StateValueError::InvalidValue
        } else {
            StateValueError::Limit {
                bytes: 625,
                maximum,
            }
        };
        assert_eq!(scanned.into_owned(maximum), Err(error));
    }
    let owner =
        scan_record_point(&bytes, &expected, &partition, &id, limits, 269)?.into_owned(625)?;
    assert_eq!(
        encode_record_fields(
            &expected,
            &partition,
            &owner.record,
            owner.ordering,
            AdmissionLimits::default(),
            269
        )?,
        bytes
    );
    assert_eq!(
        encode_record_fields(
            &expected,
            &partition,
            &owner.record,
            owner.ordering,
            AdmissionLimits::default(),
            268
        ),
        Err(StateValueError::Limit {
            bytes: 269,
            maximum: 268
        })
    );
    assert_eq!(
        encode_record_fields(
            &expected,
            &partition,
            &owner.record,
            owner.ordering,
            AdmissionLimits::default(),
            0
        ),
        Err(StateValueError::InvalidValue)
    );
    drop(bytes);
    assert_running(&owner)?;
    let bytes = hex(RUNNING)?;
    assert!(matches!(
        scan_record_point(&bytes, &expected, &partition, &id, limits, 268),
        Err(StateValueError::Limit {
            bytes: 269,
            maximum: 268
        })
    ));
    for limits in [
        AdmissionLimits::new(22, 108, 1)?,
        AdmissionLimits::new(23, 107, 1)?,
    ] {
        assert!(matches!(
            scan_record_point(&bytes, &expected, &partition, &id, limits, 269),
            Err(StateValueError::InvalidValue)
        ));
    }
    Ok(())
}

#[test]
fn outcome_scan_preserves_metadata_and_admits_owned_fields() -> TestResult {
    let bytes = hex(OUTCOME)?;
    assert_eq!(bytes.len(), 117);
    let (expected, request) = outcome_query()?;
    let limits = AdmissionLimits::new(1, 1, 1)?;
    for maximum in [0, 526] {
        let scanned = scan_outcome(&bytes, &expected, &request, limits, 117)?;
        assert_eq!(scanned.owned_bytes(), 527);
        let error = if maximum == 0 {
            StateValueError::InvalidValue
        } else {
            StateValueError::Limit {
                bytes: 527,
                maximum,
            }
        };
        assert_eq!(scanned.into_owned(maximum), Err(error));
    }
    let scanned = scan_outcome(&bytes, &expected, &request, limits, 117)?;
    drop(expected);
    drop(request);
    let owner = scanned.into_owned(527)?;
    drop(bytes);
    let (expected, request) = outcome_query()?;
    assert_eq!(owner.scope(), &expected);
    assert_eq!(owner.request(), &request);
    assert_eq!(owner.intent(), ClientIntent::record_batch([0x11; 32]));
    assert_eq!(
        owner.original_position(),
        LogIdentity::new(3, 4, AppliedPosition::new(0))
    );
    assert_eq!(owner.original_time(), TimestampMillis::new(5));
    assert_eq!(owner.expires_at(), TimestampMillis::new(105));
    assert_eq!(owner.response_version(), 1);
    assert_eq!(owner.status(), RetainedStatus::Accepted);
    assert_eq!(owner.response(), [0]);
    let bytes = hex(OUTCOME)?;
    assert_eq!(
        owner,
        decode_outcome(&bytes, &expected, &request, limits, 117)?
    );
    assert!(matches!(
        scan_outcome(&bytes, &expected, &request, limits, 116),
        Err(StateValueError::Limit {
            bytes: 117,
            maximum: 116
        })
    ));
    for field in 0..4 {
        let foreign = scope(
            if field == 0 { b"other" } else { b"t\xff" },
            if field == 1 { b"other" } else { b"g\0\xff" },
            if field == 2 { b"other" } else { b"z\0\xffx" },
        )?;
        let query = RequestId::new(if field == 3 {
            b"other".to_vec()
        } else {
            b"q\0\xffxy".to_vec()
        })?;
        assert!(matches!(
            scan_outcome(&bytes, &foreign, &query, limits, 117),
            Err(StateValueError::InvalidValue)
        ));
    }
    Ok(())
}

#[test]
fn borrowed_scans_reject_corruption_before_materialization() -> TestResult {
    let bytes = hex(RUNNING)?;
    let (expected, partition, id) = record_query()?;
    let limits = AdmissionLimits::default();
    for end in 0..bytes.len() {
        assert!(scan_record_point(&bytes[..end], &expected, &partition, &id, limits, 269).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    let foreign = scope(b"other", b"g", b"z")?;
    assert!(matches!(
        scan_record_point(&trailing, &foreign, &partition, &id, limits, 270),
        Err(StateValueError::Malformed)
    ));
    assert!(matches!(
        scan_record_point(&trailing, &expected, &partition, &id, limits, 270),
        Err(StateValueError::Malformed)
    ));
    let mut flag = bytes.clone();
    flag[114] = 2;
    assert!(matches!(
        scan_record_point(&flag, &expected, &partition, &id, limits, 269),
        Err(StateValueError::UnsupportedTag)
    ));
    let mut witness = bytes;
    witness[268] = 8;
    assert!(matches!(
        scan_record_point(&witness, &expected, &partition, &id, limits, 269),
        Err(StateValueError::InvalidValue)
    ));
    let bytes = hex(OUTCOME)?;
    let (expected, request) = outcome_query()?;
    for end in 0..bytes.len() {
        assert!(scan_outcome(&bytes[..end], &expected, &request, limits, 117).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    let foreign = scope(b"other", b"g\0\xff", b"z\0\xffx")?;
    assert!(matches!(
        scan_outcome(&trailing, &foreign, &request, limits, 118),
        Err(StateValueError::Malformed)
    ));
    assert!(matches!(
        scan_outcome(&trailing, &expected, &request, limits, 118),
        Err(StateValueError::Malformed)
    ));
    for original in [&bytes, &witness] {
        let mut version = original.clone();
        version[5] = 2;
        if version.len() == 117 {
            assert!(matches!(
                scan_outcome(&version, &expected, &request, limits, 117),
                Err(StateValueError::UnsupportedVersion)
            ));
        } else {
            let (expected, partition, id) = record_query()?;
            assert!(matches!(
                scan_record_point(&version, &expected, &partition, &id, limits, 269),
                Err(StateValueError::UnsupportedVersion)
            ));
        }
    }
    Ok(())
}

#[test]
fn borrowed_scans_preserve_absence_empty_and_legacy_decoders() -> TestResult {
    let bytes = hex(EMPTY_PAYLOAD)?;
    assert_eq!(bytes.len(), 147);
    let expected = scope(b"t\0\xff", b"g\0\xff", b"z\0\xff")?;
    let partition = PartitionId::new(b"p\0\xff".to_vec())?;
    let id = RecordId::new(b"i\0\xff".to_vec())?;
    let limits = AdmissionLimits::default();
    let scanned = scan_record_point(&bytes, &expected, &partition, &id, limits, 147)?;
    assert_eq!(scanned.owned_bytes(), 527);
    let owner = scanned.into_owned(527)?;
    assert_eq!(
        owner,
        decode_record(&bytes, expected.group(), expected.recovery(), limits, 147)?
    );
    drop(bytes);
    assert_eq!(owner.record.payload(), Some(b"".as_slice()));
    assert_eq!(owner.record.result(), None);
    assert_eq!(owner.record.error(), None);
    assert_eq!(owner.record.run_state(), None);
    assert_eq!(owner.record.lease(), None);
    assert_eq!(owner.record.next_run_at(), Some(TimestampMillis::new(7)));
    assert_eq!(owner.ordering.state_enter_seq, u128::MAX - 7);
    let mut wide = hex(EMPTY_PAYLOAD)?;
    wide[113..121].copy_from_slice(&(MAX_EXACT_MILLIS + 100).to_be_bytes());
    let scanned = scan_record_point(&wide, &expected, &partition, &id, limits, 147)?;
    assert_eq!(scanned.owned_bytes(), 527);
    let wide_owner = scanned.into_owned(527)?;
    assert_eq!(
        wide_owner.record.next_run_at(),
        Some(TimestampMillis::new(MAX_EXACT_MILLIS + 100))
    );
    assert_eq!(
        wide_owner,
        decode_record(&wide, expected.group(), expected.recovery(), limits, 147)?
    );
    let bytes = hex(RUNNING)?;
    let (expected, partition, id) = record_query()?;
    assert_eq!(
        scan_record_point(&bytes, &expected, &partition, &id, limits, 269)?.into_owned(625)?,
        decode_record(&bytes, expected.group(), expected.recovery(), limits, 269)?
    );
    Ok(())
}
