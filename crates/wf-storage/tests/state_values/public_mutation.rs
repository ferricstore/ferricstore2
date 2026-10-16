//! Independent public1/response2 wire and owner oracles; no backend evidence.

use wf_domain::workflow::DomainError;
use wf_domain::workflow::admission::{AdmissionError, AdmissionLimits};
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::indexed::IndexRejection;
use wf_domain::workflow::request::{
    ClientIntent, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
};
use wf_ports::log::public_mutation_retained_status;
use wf_storage::state::value::{StateValueError, decode_outcome, encode_outcome, scan_outcome};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, LogIdentity, TimestampMillis};

use super::TestResult;

// Independently packed before the implementation: FWSO1, t/g/z/q, intent1,
// digest11, log3/4/0, original5/expiry105, response2. Header ends at102.
const HEADER: &str = "4657534f000100000001740000000167000000017a000000017100011111111111111111111111111111111111111111111111111111111111111111000000000000000300000000000000040000000000000000000000000000000500000000000000690002";
const MISSING: &str = "4657534f000100000001740000000167000000017a00000001710001111111111111111111111111111111111111111111111111111111111111111100000000000000030000000000000004000000000000000000000000000000050000000000000069000200000003020000";

fn hex(value: &str) -> TestResult<Vec<u8>> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn query() -> TestResult<(RequestScope, RequestId)> {
    Ok((
        RequestScope::new(
            TenantId::new(b"t".to_vec())?,
            GroupId::new(b"g".to_vec())?,
            RecoveryId::new(b"z".to_vec())?,
        ),
        RequestId::new(b"q".to_vec())?,
    ))
}

fn missing() -> RetainedStatus {
    RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
        DomainError::NotFound,
    )))
}

fn frame(body: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = hex(HEADER)?;
    bytes.extend_from_slice(&u32::try_from(body.len())?.to_be_bytes());
    bytes.extend_from_slice(body);
    Ok(bytes)
}

fn outcome(status: RetainedStatus, body: &[u8]) -> TestResult<RetainedOutcome> {
    let (scope, request) = query()?;
    Ok(RetainedOutcome::new(
        RetainedMetadata {
            scope,
            request,
            intent: ClientIntent::new(1, [0x11; 32])?,
            original_position: LogIdentity::new(3, 4, AppliedPosition::new(0)),
            original_time: TimestampMillis::new(5),
            expires_at: TimestampMillis::new(105),
            status,
        },
        2,
        body.to_vec(),
        AdmissionLimits::default(),
    )?)
}

fn roundtrip() -> TestResult {
    let bytes = hex(MISSING)?;
    assert_eq!(frame(&[2, 0, 0])?, bytes);
    assert_eq!(bytes.len(), 109); // fixed102 + H4 + U3
    let expected = outcome(missing(), &[2, 0, 0])?;
    assert_eq!(encode_outcome(&expected, 109)?, bytes);
    let (scope, request) = query()?;
    let scanned = scan_outcome(&bytes, &scope, &request, AdmissionLimits::default(), 109)?;
    assert_eq!(scanned.response_count(), None); // scalar, never a synthetic vector
    assert_eq!(scanned.owned_bytes(), 519); // 512 + H4 + U3
    drop(scope);
    drop(request);
    let owned = scanned.into_owned(519)?;
    let (scope, request) = query()?;
    assert_eq!(
        decode_outcome(&bytes, &scope, &request, AdmissionLimits::default(), 109)?,
        owned
    );
    drop(bytes);
    assert_eq!(owned, expected);
    assert_eq!(owned.intent(), ClientIntent::new(1, [0x11; 32])?);
    assert_eq!(
        owned.original_position(),
        LogIdentity::new(3, 4, AppliedPosition::new(0))
    );
    assert_eq!(owned.original_time(), TimestampMillis::new(5));
    assert_eq!(owned.expires_at(), TimestampMillis::new(105));
    assert_eq!(owned.response_version(), 2);
    assert_eq!(owned.status(), missing());
    assert_eq!(owned.response(), [2, 0, 0]);
    Ok(())
}

fn valid_origins() -> TestResult {
    let admission = RetainedStatus::IndexedRejected(IndexRejection::Batch(
        BatchRejection::Admission(AdmissionError::DecisionBytes),
    ));
    let cross = RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::CrossGroup));
    for (body, status, wire, owner) in [
        (&[0][..], RetainedStatus::Accepted, 107, 517),
        (&[2, 0, 0], missing(), 109, 519),
        (&[1, 1, 4], admission, 109, 519),
        (&[1, 3, 0], cross, 109, 519),
    ] {
        let expected = outcome(status, body)?;
        assert_eq!(
            public_mutation_retained_status(expected.intent(), 2, body)?,
            status
        );
        let bytes = frame(body)?;
        assert_eq!(encode_outcome(&expected, wire)?, bytes);
        let (scope, request) = query()?;
        let scanned = scan_outcome(&bytes, &scope, &request, AdmissionLimits::default(), wire)?;
        assert_eq!(scanned.response_count(), None);
        assert_eq!(scanned.owned_bytes(), owner);
        assert_eq!(scanned.into_owned(owner)?, expected);
    }
    let wrong = outcome(RetainedStatus::Accepted, &[2, 0, 0])?;
    assert_eq!(
        encode_outcome(&wrong, 109),
        Err(StateValueError::InvalidValue)
    );
    let wrong = outcome(missing(), &[0])?;
    assert_eq!(
        encode_outcome(&wrong, 107),
        Err(StateValueError::InvalidValue)
    );
    Ok(())
}

fn invalid_origins() -> TestResult {
    let (scope, request) = query()?;
    for body in [
        &[][..],
        &[0, 0],
        &[1, 0, 0],
        &[1, 2, 0],
        &[1, 4, 0],
        &[1, 6, 0],
        &[2, 1, 4],
        &[2, 2, 0],
        &[2, 3, 0],
        &[2, 6, 0],
        &[2, 0, 23],
        &[2, 0, 8, 7],
        &[2, 0, 16, 3],
        &[2, 0, 19, 8],
        &[2, 0, 0, 0],
        &[2, 0, 0, 0, 1, 0],
        &[3],
    ] {
        assert!(
            public_mutation_retained_status(ClientIntent::new(1, [0x11; 32])?, 2, body).is_err()
        );
        let bytes = frame(body)?;
        assert!(
            scan_outcome(
                &bytes,
                &scope,
                &request,
                AdmissionLimits::default(),
                bytes.len()
            )
            .is_err()
        );
        let invalid = outcome(missing(), body)?;
        assert!(encode_outcome(&invalid, 1_024).is_err());
    }
    Ok(())
}

fn corruption() -> TestResult {
    let bytes = hex(MISSING)?;
    let (scope, request) = query()?;
    for end in 0..bytes.len() {
        assert!(
            scan_outcome(
                &bytes[..end],
                &scope,
                &request,
                AdmissionLimits::default(),
                109
            )
            .is_err()
        );
    }
    for (offset, value, error) in [
        (0, b'X', StateValueError::UnsupportedTag),
        (5, 2, StateValueError::UnsupportedVersion),
        (27, 5, StateValueError::UnsupportedVersion),
        (101, 1, StateValueError::UnsupportedVersion),
    ] {
        let mut bad = bytes.clone();
        bad[offset] = value;
        assert!(
            matches!(scan_outcome(&bad, &scope, &request, AdmissionLimits::default(), 109), Err(actual) if actual==error)
        );
    }
    let mut expired = bytes.clone();
    expired[92..100].copy_from_slice(&5_u64.to_be_bytes());
    assert!(matches!(
        scan_outcome(&expired, &scope, &request, AdmissionLimits::default(), 109),
        Err(StateValueError::InvalidValue)
    ));
    let mut length = bytes.clone();
    length[102..106].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(
        scan_outcome(&length, &scope, &request, AdmissionLimits::default(), 109),
        Err(StateValueError::Malformed)
    ));
    let mut trailing = bytes;
    trailing.push(0);
    let foreign = RequestScope::new(
        TenantId::new(b"other".to_vec())?,
        scope.group().clone(),
        scope.recovery().clone(),
    );
    for expected in [&scope, &foreign] {
        assert!(matches!(
            scan_outcome(
                &trailing,
                expected,
                &request,
                AdmissionLimits::default(),
                110
            ),
            Err(StateValueError::Malformed)
        ));
    }
    Ok(())
}

fn limits_and_scope() -> TestResult {
    let bytes = hex(MISSING)?;
    let (scope, request) = query()?;
    let expected = outcome(missing(), &[2, 0, 0])?;
    for result in [
        encode_outcome(&expected, 108).map(|_| ()),
        scan_outcome(&bytes, &scope, &request, AdmissionLimits::default(), 108).map(|_| ()),
    ] {
        assert_eq!(
            result,
            Err(StateValueError::Limit {
                bytes: 109,
                maximum: 108
            })
        );
    }
    let scanned = scan_outcome(&bytes, &scope, &request, AdmissionLimits::default(), 109)?;
    assert_eq!(
        scanned.into_owned(518),
        Err(StateValueError::Limit {
            bytes: 519,
            maximum: 518
        })
    );
    let scanned = scan_outcome(&bytes, &scope, &request, AdmissionLimits::default(), 109)?;
    assert_eq!(scanned.into_owned(0), Err(StateValueError::InvalidValue));
    assert_eq!(
        encode_outcome(&expected, 0),
        Err(StateValueError::InvalidValue)
    );
    assert!(matches!(
        scan_outcome(&bytes, &scope, &request, AdmissionLimits::default(), 0),
        Err(StateValueError::InvalidValue)
    ));
    for field in 0..4 {
        let foreign = RequestScope::new(
            TenantId::new(if field == 0 {
                b"other".to_vec()
            } else {
                b"t".to_vec()
            })?,
            GroupId::new(if field == 1 {
                b"other".to_vec()
            } else {
                b"g".to_vec()
            })?,
            RecoveryId::new(if field == 2 {
                b"other".to_vec()
            } else {
                b"z".to_vec()
            })?,
        );
        let request = RequestId::new(if field == 3 {
            b"other".to_vec()
        } else {
            b"q".to_vec()
        })?;
        assert!(matches!(
            scan_outcome(&bytes, &foreign, &request, AdmissionLimits::default(), 109),
            Err(StateValueError::InvalidValue)
        ));
    }
    Ok(())
}

fn maximum_identity() -> TestResult {
    let mut metadata = RetainedMetadata {
        scope: RequestScope::new(
            TenantId::new(vec![0xff; 8_192])?,
            GroupId::new(vec![0; 256])?,
            RecoveryId::new(vec![b':'; 256])?,
        ),
        request: RequestId::new(vec![0; 256])?,
        intent: ClientIntent::new(1, [0x11; 32])?,
        original_position: LogIdentity::new(3, 4, AppliedPosition::new(0)),
        original_time: TimestampMillis::new(5),
        expires_at: TimestampMillis::new(105),
        status: missing(),
    };
    let scope = metadata.scope.clone();
    let request = metadata.request.clone();
    let expected = RetainedOutcome::new(
        metadata.clone(),
        2,
        vec![2, 0, 0],
        AdmissionLimits::default(),
    )?;
    let bytes = encode_outcome(&expected, 9_065)?;
    assert_eq!(bytes.len(), 9_065); //102 + (8192+256+256+256) +3
    let scanned = scan_outcome(&bytes, &scope, &request, AdmissionLimits::default(), 9_065)?;
    assert_eq!(scanned.owned_bytes(), 9_475);
    assert_eq!(scanned.into_owned(9_475)?, expected);
    metadata.status = RetainedStatus::Accepted;
    assert!(
        RetainedOutcome::new(metadata, 2, vec![0; 4], AdmissionLimits::new(256, 3, 1)?).is_err()
    );
    Ok(())
}

#[test]
fn public_item_outcome_profile_and_operational_limits() -> TestResult {
    roundtrip()?;
    valid_origins()?;
    invalid_origins()?;
    corruption()?;
    limits_and_scope()?;
    maximum_identity()
}
