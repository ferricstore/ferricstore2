use super::TestResult;
use wf_domain::{
    claim::caller::{ClaimCallerLimits, ClaimDueInput, normalize_claim_due},
    workflow::request::RequestScope,
};
use wf_ports::log::{
    CodecLimits, PreparedBytes, PublicClaimPreparation, encode_public_claim_request,
    prepare_atomic_claim_request, prepare_public_claim_intent, prepare_public_claim_request,
};
use wf_types::{
    identity::{GroupId, RecoveryId, RequestId, TenantId},
    version::{CommittedTime, HlcTimestamp},
};

fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t\0".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    ))
}
fn frame(now: u64, batch: [u8; 16]) -> TestResult<PreparedBytes> {
    let caller = normalize_claim_due(
        ClaimDueInput::new(b"type", b"worker"),
        ClaimCallerLimits::default(),
    )?;
    Ok(encode_public_claim_request(
        &scope()?,
        &RequestId::new(b"request".to_vec())?,
        CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?,
        &caller,
        PublicClaimPreparation {
            token_version: 1,
            first_ordinal: 0,
            scan_budget: 100,
            head_budget: 100,
            batch,
        },
        CodecLimits::default(),
        1_048_576,
    )?)
}

#[test]
fn actual_public_frame_binds_v1_caller_but_keeps_generated_batch_and_time_out_of_intent()
-> TestResult {
    let first = frame(100, [1; 16])?;
    let later = frame(101, [2; 16])?;
    assert_eq!(&first.as_bytes()[..7], b"FWCP\0\x01\x13");
    assert_eq!(&first.as_bytes()[first.as_bytes().len() - 16..], &[1; 16]);
    let prepared = prepare_public_claim_request(
        first.as_bytes(),
        CodecLimits::default(),
        ClaimCallerLimits::default(),
        1_048_576,
    )?;
    let changed = prepare_public_claim_request(
        later.as_bytes(),
        CodecLimits::default(),
        ClaimCallerLimits::default(),
        1_048_576,
    )?;
    let root = prepared.prepared();
    assert_eq!(root.as_bytes(), first.as_bytes());
    assert_eq!(root.scope(), &scope()?);
    assert_eq!(root.intent().version(), 21);
    assert!(root.payment().is_none());
    let call = root.caller(ClaimCallerLimits::default())?;
    let public = prepare_public_claim_intent(scope()?.tenant(), &call, 65536)?;
    assert_eq!(root.intent().digest(), public.digest());
    assert_eq!(root.intent(), changed.prepared().intent());
    assert_ne!(root.digest(), changed.prepared().digest());
    assert!(
        prepare_atomic_claim_request(
            first.as_bytes(),
            CodecLimits::default(),
            ClaimCallerLimits::default(),
            1_048_576
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn funded_parent_requires_complete_original_nonce_header_caller_and_eof() -> TestResult {
    let original = frame(100, [0xff; 16])?;
    for end in 0..original.as_bytes().len() {
        assert!(
            prepare_public_claim_request(
                &original.as_bytes()[..end],
                CodecLimits::default(),
                ClaimCallerLimits::default(),
                1_048_576
            )
            .is_err(),
            "truncated at {end}"
        );
    }
    let mut changed = original.as_bytes().to_vec();
    changed.push(0);
    assert!(
        prepare_public_claim_request(
            &changed,
            CodecLimits::default(),
            ClaimCallerLimits::default(),
            1_048_576
        )
        .is_err()
    );
    assert!(
        prepare_public_claim_request(
            original.as_bytes(),
            CodecLimits::default(),
            ClaimCallerLimits::default(),
            1
        )
        .is_err()
    );
    changed = original.as_bytes().to_vec();
    changed[6] = 18;
    assert!(
        prepare_public_claim_request(
            &changed,
            CodecLimits::default(),
            ClaimCallerLimits::default(),
            1_048_576
        )
        .is_err()
    );
    Ok(())
}
