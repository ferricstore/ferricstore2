//! Restoration-format controls; no actual store or full source execution claim.

use super::TestResult;
use sha2::{Digest, Sha256};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::{IndexRejection, IndexedDecision};
use wf_domain::workflow::request::{
    ClientIntent, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
};
use wf_ports::log::encode_step_continue_response;
use wf_storage::state::value::{
    StateValueError, encode_outcome, encode_step_continue_outcome, scan_outcome,
    scan_step_continue_outcome,
};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, LogIdentity, TimestampMillis};

// Independent FWSO framing of the previously verified 160-byte FWST fixture.
const ACCEPTED: &str = concat!(
    "4657534f000100000001740000000167000000017a0000000171000a111111111111111111111111",
    "11111111111111111111111111111111111111110000000000000001000000000000000700000000",
    "00000009000000000000000200000000000000660009000000a04657535400010000000000000001",
    "00000000000000070000000000000009000000000000000200000000017000000001690000000565",
    "6d61696c00000006636861726765000000037700ff000000077700ff3a323a320000000000000003",
    "00000000000000020000000000007532000000000000000000000000000000020000000000000000",
    "0100000000000000000000000900000003010000000200ff0000"
);
const REJECTED: &str = concat!(
    "4657534f000100000001740000000167000000017a0000000171000a111111111111111111111111",
    "11111111111111111111111111111111111111110000000000000001000000000000000700000000",
    "00000009000000000000000200000000000000660009000000294657535400010000000000000001",
    "000000000000000700000000000000090000000000000002010600"
);

fn hex(value: &str) -> TestResult<Vec<u8>> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn limits() -> TestResult<AdmissionLimits> {
    Ok(AdmissionLimits::new(32, 8192, 4)?)
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

fn metadata(status: RetainedStatus) -> TestResult<RetainedMetadata> {
    let (scope, request) = query()?;
    Ok(RetainedMetadata {
        scope,
        request,
        intent: ClientIntent::step_continue([0x11; 32]),
        original_position: LogIdentity::new(1, 7, AppliedPosition::new(9)),
        original_time: TimestampMillis::new(2),
        expires_at: TimestampMillis::new(102),
        status,
    })
}

fn outcome(accepted: bool) -> TestResult<RetainedOutcome> {
    let (response, status) = if accepted {
        (
            hex(ACCEPTED)?
                .get(106..)
                .ok_or("fixture response")?
                .to_vec(),
            RetainedStatus::Accepted,
        )
    } else {
        encode_step_continue_response(
            &IndexedDecision::Rejected(IndexRejection::NotFifoHead),
            LogIdentity::new(1, 7, AppliedPosition::new(9)),
            TimestampMillis::new(2),
            limits()?,
        )?
    };
    Ok(RetainedOutcome::new(
        metadata(status)?,
        9,
        response,
        limits()?,
    )?)
}

fn encoded(outcome: &RetainedOutcome) -> TestResult<Vec<u8>> {
    let result = encode_step_continue_outcome(outcome, 8192, limits()?);
    assert!(
        result.is_ok(),
        "healthy Step receipt encode: {:?}",
        result.as_ref().err()
    );
    Ok(result?)
}

#[test]
fn step_receipt_writer_preserves_original_fields_and_golden() -> TestResult {
    let (scope, request) = query()?;
    for (accepted, golden, digest) in [
        (
            true,
            ACCEPTED,
            "5e263ff7d8f6e76a507437a64bf16d399cc9c844211542cd6ccd31ae5c9372ee",
        ),
        (
            false,
            REJECTED,
            "7e658db85c06b724048c969d000940afc9aa7e557f3379035fe9ea9b67fe1c54",
        ),
    ] {
        let expected = outcome(accepted)?;
        let bytes = encoded(&expected)?;
        assert_eq!(bytes, hex(golden)?);
        assert_eq!(Sha256::digest(&bytes).as_slice(), hex(digest)?);
        assert!(encode_outcome(&expected, 8192).is_err());
        assert!(matches!(
            scan_outcome(&bytes, &scope, &request, limits()?, 8192),
            Err(StateValueError::UnsupportedVersion)
        ));
        let scanned = scan_step_continue_outcome(&bytes, &scope, &request, limits()?, 8192)?;
        assert_eq!(scanned.response_count(), accepted.then_some(1));
        let restored = scanned.into_owned(8192)?;
        assert_eq!(restored, expected);
        assert_eq!(
            restored.original_position(),
            LogIdentity::new(1, 7, AppliedPosition::new(9))
        );
        assert_eq!(
            (restored.original_time().get(), restored.expires_at().get()),
            (2, 102)
        );
        assert_eq!(restored.intent(), ClientIntent::step_continue([0x11; 32]));
    }
    let expected = outcome(true)?;
    let wrong = RetainedOutcome::new(
        metadata(RetainedStatus::IndexedRejected(IndexRejection::NotFifoHead))?,
        9,
        expected.response().to_vec(),
        limits()?,
    )?;
    assert_eq!(
        encode_step_continue_outcome(&wrong, 8192, limits()?),
        Err(StateValueError::InvalidValue)
    );
    Ok(())
}

fn corruptions(bytes: &[u8]) -> TestResult {
    let (scope, request) = query()?;
    for end in 0..bytes.len() {
        assert!(
            scan_step_continue_outcome(&bytes[..end], &scope, &request, limits()?, 8192).is_err(),
            "prefix {end}"
        );
    }
    let mut trailing = bytes.to_vec();
    trailing.push(0);
    assert!(scan_step_continue_outcome(&trailing, &scope, &request, limits()?, 8192).is_err());
    for offset in [
        0, 5, 10, 15, 20, 25, 27, 67, 75, 83, 91, 100, 102, 106, 111, 257, 264, 265,
    ] {
        let mut changed = bytes.to_vec();
        changed[offset] ^= 0xff;
        assert!(
            scan_step_continue_outcome(&changed, &scope, &request, limits()?, 8192).is_err(),
            "offset {offset}"
        );
    }
    let mut expired_at_birth = bytes.to_vec();
    expired_at_birth[99] = 1;
    assert_eq!(
        scan_step_continue_outcome(&expired_at_birth, &scope, &request, limits()?, 8192).err(),
        Some(StateValueError::InvalidValue)
    );
    Ok(())
}

#[test]
fn step_receipt_reader_owns_fields_and_rejects_complete_context_corruption() -> TestResult {
    let mut bytes = hex(ACCEPTED)?;
    let (scope, request) = query()?;
    let first = scan_step_continue_outcome(&bytes, &scope, &request, limits()?, 8192);
    assert!(
        first.is_ok(),
        "healthy Step receipt scan: {:?}",
        first.as_ref().err()
    );
    let scanned = first?;
    assert_eq!(scanned.owned_bytes(), 676);
    assert_eq!(scanned.response_count(), Some(1));
    drop(scope);
    drop(request);
    let restored = scanned.into_owned(676)?;
    bytes.fill(0);
    drop(bytes);
    assert_eq!(restored, outcome(true)?);
    corruptions(&hex(ACCEPTED)?)?;
    Ok(())
}

fn exact_limits(expected: &RetainedOutcome, bytes: &[u8], owner: usize) -> TestResult {
    let (scope, request) = query()?;
    assert_eq!(
        encode_step_continue_outcome(expected, bytes.len(), limits()?)?,
        bytes
    );
    assert_eq!(
        encode_step_continue_outcome(expected, bytes.len() - 1, limits()?).err(),
        Some(StateValueError::Limit {
            bytes: bytes.len(),
            maximum: bytes.len() - 1
        })
    );
    assert_eq!(
        scan_step_continue_outcome(bytes, &scope, &request, limits()?, bytes.len() - 1).err(),
        Some(StateValueError::Limit {
            bytes: bytes.len(),
            maximum: bytes.len() - 1
        })
    );
    let scanned = scan_step_continue_outcome(bytes, &scope, &request, limits()?, bytes.len())?;
    assert_eq!(scanned.owned_bytes(), owner);
    assert_eq!(
        scanned.into_owned(owner - 1).err(),
        Some(StateValueError::Limit {
            bytes: owner,
            maximum: owner - 1
        })
    );
    assert_eq!(
        scan_step_continue_outcome(bytes, &scope, &request, limits()?, bytes.len())?
            .into_owned(owner)?,
        *expected
    );
    assert_eq!(
        encode_step_continue_outcome(expected, 0, limits()?),
        Err(StateValueError::InvalidValue)
    );
    assert_eq!(
        scan_step_continue_outcome(bytes, &scope, &request, limits()?, 0).err(),
        Some(StateValueError::InvalidValue)
    );
    Ok(())
}

#[test]
fn step_receipt_explicit_profile_and_independent_limits_refuse() -> TestResult {
    let expected = outcome(true)?;
    let bytes = encoded(&expected)?;
    exact_limits(&expected, &bytes, 676)?;
    let rejected = outcome(false)?;
    exact_limits(&rejected, &encoded(&rejected)?, 557)?;
    let (scope, request) = query()?;
    let exact = AdmissionLimits::new(7, 160, 4)?;
    assert_eq!(encode_step_continue_outcome(&expected, 266, exact)?, bytes);
    assert_eq!(
        scan_step_continue_outcome(&bytes, &scope, &request, exact, 266)?.into_owned(676)?,
        expected
    );
    for narrow in [
        AdmissionLimits::new(6, 160, 4)?,
        AdmissionLimits::new(7, 159, 4)?,
    ] {
        assert_eq!(
            encode_step_continue_outcome(&expected, 8192, narrow),
            Err(StateValueError::ProfileLimit)
        );
        assert_eq!(
            scan_step_continue_outcome(&bytes, &scope, &request, narrow, 8192).err(),
            Some(StateValueError::ProfileLimit)
        );
    }
    let mut metadata = metadata(RetainedStatus::Accepted)?;
    metadata.intent = ClientIntent::record_batch([0x11; 32]);
    let legacy = RetainedOutcome::new(metadata, 1, vec![0], limits()?)?;
    assert_eq!(
        encode_step_continue_outcome(&legacy, 8192, limits()?),
        Err(StateValueError::UnsupportedVersion)
    );
    let legacy = encode_outcome(&legacy, 8192)?;
    assert_eq!(
        scan_step_continue_outcome(&legacy, &scope, &request, limits()?, 8192).err(),
        Some(StateValueError::UnsupportedVersion)
    );
    Ok(())
}
