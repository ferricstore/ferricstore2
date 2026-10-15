//! Independent FWSO values retain one canonical response, deriving its status.

use wf_domain::value::{Counter, DomainError, InputField, RetryPolicyField};
use wf_domain::workflow::admission::{AdmissionError, AdmissionLimits};
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::indexed::IndexRejection;
use wf_domain::workflow::request::{
    ClientIntent, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
};
use wf_ports::log::{CodecError, record_batch_retained_status};
use wf_storage::state::value::{StateValueError, decode_outcome, encode_outcome};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, LogIdentity, TimestampMillis};

use super::TestResult;

const ACCEPTED: &str = "4657534f00010000000174000000016700000001720000000171000511111111111111111111111111111111111111111111111111111111111111110000000000000003000000000000000400000000000000000000000000000005000000000000006900010000000100";
const REJECTED: &str = "4657534f00010000000174000000016700000001720000000171000511111111111111111111111111111111111111111111111111111111111111110000000000000003000000000000000400000000000000000000000000000005000000000000006900010000001301010300000000000000150000000000000014";

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

fn outcome(
    scope: RequestScope,
    request: RequestId,
    status: RetainedStatus,
    response: Vec<u8>,
) -> TestResult<RetainedOutcome> {
    Ok(RetainedOutcome::new(
        RetainedMetadata {
            scope,
            request,
            intent: ClientIntent::record_batch([0x11; 32]),
            original_position: LogIdentity::new(3, 4, AppliedPosition::new(0)),
            original_time: TimestampMillis::new(5),
            expires_at: TimestampMillis::new(105),
            status,
        },
        1,
        response,
        AdmissionLimits::default(),
    )?)
}

fn frame(response: &[u8]) -> TestResult<Vec<u8>> {
    // The independent literal's header ends before the response length at102.
    let mut bytes = hex(ACCEPTED)?;
    bytes.truncate(102);
    bytes.extend_from_slice(&u32::try_from(response.len())?.to_be_bytes());
    bytes.extend_from_slice(response);
    Ok(bytes)
}

fn assert_response(response: &[u8], status: RetainedStatus) -> TestResult {
    let scope = scope(b"t", b"g", b"r")?;
    let request = RequestId::new(b"q".to_vec())?;
    assert_eq!(
        record_batch_retained_status(ClientIntent::record_batch([0x11; 32]), 1, response)?,
        status
    );
    let expected = outcome(scope.clone(), request.clone(), status, response.to_vec())?;
    let bytes = encode_outcome(&expected, 16_384)?;
    assert_eq!(bytes, frame(response)?);
    let restored = decode_outcome(
        &bytes,
        &scope,
        &request,
        AdmissionLimits::default(),
        bytes.len(),
    )?;
    drop(bytes);
    assert_eq!(restored, expected);
    Ok(())
}

fn domain_statuses() -> TestResult {
    for (code, error) in [
        (0, DomainError::NotFound),
        (1, DomainError::AlreadyExists),
        (2, DomainError::WrongState),
        (3, DomainError::WrongVersion),
        (4, DomainError::StaleLease),
        (5, DomainError::Terminal),
        (6, DomainError::RunningTransition),
        (7, DomainError::TerminalTransition),
        (9, DomainError::NowTooLarge),
        (10, DomainError::RunAtTooLarge),
        (11, DomainError::ExpectedVersionTooLarge),
        (12, DomainError::FenceTooLarge),
        (13, DomainError::ZeroLease),
        (14, DomainError::LeaseTooLarge),
        (15, DomainError::LeaseDeadlineTooLarge),
        (17, DomainError::ReservedId),
        (18, DomainError::ReservedType),
        (20, DomainError::PolicyTypeMismatch),
        (21, DomainError::RetryDeadlineOverflow),
        (22, DomainError::UnsupportedOperation),
    ] {
        assert_response(
            &[1, 0, code],
            RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(error))),
        )?;
    }
    for (detail, field) in [
        (0, InputField::Type),
        (1, InputField::State),
        (2, InputField::From),
        (3, InputField::To),
        (4, InputField::Worker),
        (5, InputField::LeaseToken),
        (6, InputField::ExpectState),
    ] {
        assert_response(
            &[1, 0, 8, detail],
            RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
                DomainError::EmptyValue(field),
            ))),
        )?;
    }
    for (detail, field) in [
        (0, Counter::Version),
        (1, Counter::Fence),
        (2, Counter::Attempts),
    ] {
        assert_response(
            &[1, 0, 16, detail],
            RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
                DomainError::CounterOverflow(field),
            ))),
        )?;
    }
    for (detail, field) in [
        (0, RetryPolicyField::MaxRetries),
        (1, RetryPolicyField::BaseMillis),
        (2, RetryPolicyField::MaxMillis),
        (3, RetryPolicyField::JitterPercent),
        (4, RetryPolicyField::ExhaustedTo),
        (5, RetryPolicyField::RunningExhaustion),
        (6, RetryPolicyField::State),
        (7, RetryPolicyField::RunningState),
    ] {
        assert_response(
            &[1, 0, 19, detail],
            RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
                DomainError::InvalidRetryPolicy(field),
            ))),
        )?;
    }
    Ok(())
}

fn other_statuses() -> TestResult {
    for (code, error) in [
        (0, AdmissionError::InvalidLimits),
        (1, AdmissionError::BatchItems),
        (2, AdmissionError::KeyTooLarge),
        (4, AdmissionError::DecisionBytes),
        (5, AdmissionError::LengthOverflow),
        (6, AdmissionError::EncodedValueCount),
    ] {
        assert_response(
            &[1, 1, code],
            RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Admission(
                error,
            ))),
        )?;
    }
    for (code, error) in [
        (2, IndexRejection::Batch(BatchRejection::DuplicateId)),
        (3, IndexRejection::Batch(BatchRejection::CrossGroup)),
        (4, IndexRejection::InvalidPriority),
        (5, IndexRejection::FifoPriority),
        (6, IndexRejection::NotFifoHead),
    ] {
        assert_response(&[1, code, 0], RetainedStatus::IndexedRejected(error))?;
    }
    Ok(())
}

#[test]
fn batch5_outcome_vectors_derive_exact_response_status() -> TestResult {
    assert_response(&[0], RetainedStatus::Accepted)?;
    assert_eq!(frame(&[0])?, hex(ACCEPTED)?);
    let rejected = RetainedStatus::IndexedRejected(IndexRejection::Batch(
        BatchRejection::Admission(AdmissionError::ValueTooLarge {
            bytes: 21,
            maximum: 20,
        }),
    ));
    let mut body = vec![1, 1, 3];
    body.extend_from_slice(&21_u64.to_be_bytes());
    body.extend_from_slice(&20_u64.to_be_bytes());
    assert_response(&body, rejected)?;
    assert_eq!(frame(&body)?, hex(REJECTED)?);
    assert_eq!(hex(ACCEPTED)?.len(), 107);
    assert_eq!(hex(REJECTED)?.len(), 125);
    domain_statuses()?;
    other_statuses()?;
    let max_scope = scope(&[0xff; 8_192], &[0; 256], &[b':'; 256])?;
    let request = RequestId::new(vec![0; 256])?;
    let expected = outcome(max_scope.clone(), request.clone(), rejected, body)?;
    let bytes = encode_outcome(&expected, 16_384)?;
    assert_eq!(bytes.len(), 9_081);
    assert_eq!(
        decode_outcome(
            &bytes,
            &max_scope,
            &request,
            AdmissionLimits::default(),
            9_081
        )?,
        expected
    );
    Ok(())
}

fn malformed_responses(scope: &RequestScope, request: &RequestId) -> TestResult {
    for response in [
        &[][..],
        &[2],
        &[1],
        &[1, 0],
        &[1, 7, 0],
        &[1, 0, 23],
        &[1, 0, 8, 7],
        &[1, 0, 16, 3],
        &[1, 0, 19, 8],
        &[0, 0],
        &[1, 1, 3],
        &[1, 2, 1],
        &[1, 1, 7],
    ] {
        assert!(
            record_batch_retained_status(ClientIntent::record_batch([0x11; 32]), 1, response)
                .is_err()
        );
        assert!(
            decode_outcome(
                &frame(response)?,
                scope,
                request,
                AdmissionLimits::default(),
                16_384
            )
            .is_err()
        );
    }
    let mut wrong_intent = hex(ACCEPTED)?;
    wrong_intent[27] = 1;
    let mut wrong_response = hex(ACCEPTED)?;
    wrong_response[101] = 2;
    for bytes in [&wrong_intent, &wrong_response] {
        assert!(matches!(
            decode_outcome(bytes, scope, request, AdmissionLimits::default(), 16_384),
            Err(StateValueError::UnsupportedVersion)
        ));
    }
    assert!(matches!(
        record_batch_retained_status(ClientIntent::new(1, [0x11; 32])?, 1, &[0]),
        Err(CodecError::UnsupportedVersion)
    ));
    assert!(matches!(
        record_batch_retained_status(ClientIntent::record_batch([0x11; 32]), 2, &[0]),
        Err(CodecError::UnsupportedVersion)
    ));
    Ok(())
}

fn outcome_limits(scope: &RequestScope, request: &RequestId) -> TestResult {
    let bytes = hex(ACCEPTED)?;
    let expected = outcome(
        scope.clone(),
        request.clone(),
        RetainedStatus::Accepted,
        vec![0],
    )?;
    let legacy = RetainedOutcome::new(
        RetainedMetadata {
            scope: scope.clone(),
            request: request.clone(),
            intent: ClientIntent::new(1, [0x11; 32])?,
            original_position: expected.original_position(),
            original_time: expected.original_time(),
            expires_at: expected.expires_at(),
            status: RetainedStatus::Accepted,
        },
        1,
        vec![0],
        AdmissionLimits::default(),
    )?;
    assert!(matches!(
        encode_outcome(&legacy, 16_384),
        Err(StateValueError::UnsupportedVersion)
    ));
    assert_eq!(encode_outcome(&expected, 107)?, bytes);
    for result in [
        encode_outcome(&expected, 106).map(|_| ()),
        decode_outcome(&bytes, scope, request, AdmissionLimits::default(), 106).map(|_| ()),
    ] {
        assert!(matches!(
            result,
            Err(StateValueError::Limit {
                bytes: 107,
                maximum: 106
            })
        ));
    }
    assert!(matches!(
        encode_outcome(&expected, 0),
        Err(StateValueError::InvalidValue)
    ));
    assert!(matches!(
        decode_outcome(&bytes, scope, request, AdmissionLimits::default(), 0),
        Err(StateValueError::InvalidValue)
    ));
    for status in [
        RetainedStatus::IndexedRejected(IndexRejection::NotFifoHead),
        RetainedStatus::Rejected(BatchRejection::DuplicateId),
    ] {
        let mismatch = outcome(scope.clone(), request.clone(), status, vec![0])?;
        assert!(matches!(
            encode_outcome(&mismatch, 16_384),
            Err(StateValueError::InvalidValue)
        ));
    }
    Ok(())
}

#[test]
fn outcome_rejects_foreign_profile_scope_and_corrupt_response() -> TestResult {
    let expected_scope = scope(b"t", b"g", b"r")?;
    let request = RequestId::new(b"q".to_vec())?;
    let bytes = hex(ACCEPTED)?;
    for prefix in 0..bytes.len() {
        assert!(
            decode_outcome(
                &bytes[..prefix],
                &expected_scope,
                &request,
                AdmissionLimits::default(),
                16_384
            )
            .is_err()
        );
    }
    for (offset, value) in [(0, b'X'), (5, 2), (99, 5)] {
        let mut changed = bytes.clone();
        changed[offset] = value;
        assert!(
            decode_outcome(
                &changed,
                &expected_scope,
                &request,
                AdmissionLimits::default(),
                16_384
            )
            .is_err()
        );
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        decode_outcome(
            &trailing,
            &expected_scope,
            &request,
            AdmissionLimits::default(),
            16_384
        ),
        Err(StateValueError::Malformed)
    ));
    for foreign in [
        scope(b"other", b"g", b"r")?,
        scope(b"t", b"other", b"r")?,
        scope(b"t", b"g", b"other")?,
    ] {
        assert!(matches!(
            decode_outcome(
                &bytes,
                &foreign,
                &request,
                AdmissionLimits::default(),
                16_384
            ),
            Err(StateValueError::InvalidValue)
        ));
    }
    assert!(matches!(
        decode_outcome(
            &bytes,
            &expected_scope,
            &RequestId::new(b"other".to_vec())?,
            AdmissionLimits::default(),
            16_384
        ),
        Err(StateValueError::InvalidValue)
    ));
    malformed_responses(&expected_scope, &request)?;
    outcome_limits(&expected_scope, &request)?;
    Ok(())
}
