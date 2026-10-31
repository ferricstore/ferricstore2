use super::TestResult;
use wf_domain::claim::caller::{
    ClaimCallerLimits, ClaimDueInput, ClaimPartitionsInput, normalize_claim_due,
};
use wf_ports::log::{CodecError, prepare_public_claim_intent};
use wf_types::identity::TenantId;

#[test]
fn original_aliases_and_block_attempts_share_public_identity_without_preparation() -> TestResult {
    let tenant = TenantId::new(b"tenant".to_vec())?;
    let baseline = normalize_claim_due(
        ClaimDueInput::new(b"type", b"worker"),
        ClaimCallerLimits::default(),
    )?;
    let mut input = ClaimDueInput::new(b"type", b"worker");
    input.partitions = ClaimPartitionsInput::One(b"aUtO");
    input.block_ms = Some(100);
    let alias = normalize_claim_due(input, ClaimCallerLimits::default())?;
    let original = prepare_public_claim_intent(&tenant, &baseline, 65536)?;
    assert_eq!(original.version(), 1);
    assert_eq!(
        prepare_public_claim_intent(&tenant, &alias, 65536)?,
        original
    );
    input.requested_now_ms = Some(0);
    let changed = normalize_claim_due(input, ClaimCallerLimits::default())?;
    assert_ne!(
        prepare_public_claim_intent(&tenant, &changed, 65536)?,
        original
    );
    assert_ne!(
        prepare_public_claim_intent(&TenantId::new(b"other".to_vec())?, &baseline, 65536)?,
        original
    );
    Ok(())
}

#[test]
fn tiny_public_byte_budget_refuses_before_any_canonical_owner_is_built() -> TestResult {
    let caller = normalize_claim_due(
        ClaimDueInput::new(b"type", b"worker"),
        ClaimCallerLimits::default(),
    )?;
    assert_eq!(
        prepare_public_claim_intent(&TenantId::new(b"tenant".to_vec())?, &caller, 1),
        Err(CodecError::ByteLimit)
    );
    Ok(())
}
