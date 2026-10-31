use super::{fixtures::*, *};

#[test]
fn invalid_submitted_reply_and_unresolved_retry_keep_the_original_context() -> TestResult {
    let (scope, id, caller) = (scope()?, id()?, caller()?);
    let mut port = ClaimSubmitFixture::new(ClaimPortMode::Fresh);
    let foreign = RequestId::new(b"foreign".to_vec())?;
    port.reply = Some(M::Retained(outcome(&scope, &foreign, &caller)?));
    let PublicClaimCallResult::Unknown {
        reason: U::InvalidReply,
        original,
    } = submit_public_claim_call(
        PublicClaimCallRequest {
            scope: &scope,
            request: &id,
            caller: &caller,
        },
        &mut port,
        PublicClaimCallLimits::default(),
    )?
    else {
        return Err("invalid reply preserves original".into());
    };
    let pointer = original.frame().as_ptr();
    port.mode = ClaimPortMode::Unknown;
    let PublicClaimCallResult::Unknown {
        reason: U::Unavailable,
        original,
    } = retry_public_claim_call(original, &mut port, PublicClaimCallLimits::default())?
    else {
        return Err("unresolved lookup preserves original".into());
    };
    assert_eq!(original.frame().as_ptr(), pointer);
    assert_eq!(
        port.trace,
        [
            ClaimTrace::Lookup,
            ClaimTrace::Prepare,
            ClaimTrace::Submit,
            ClaimTrace::Lookup
        ]
    );
    assert_eq!(port.submitted.len(), 1);
    Ok(())
}
#[test]
fn unknown_retry_looks_up_first_and_resubmits_the_same_arc_without_new_entropy() -> TestResult {
    let (scope, id, caller) = (scope()?, id()?, caller()?);
    let mut port = ClaimSubmitFixture::new(ClaimPortMode::Fresh);
    let PublicClaimCallResult::Unknown { original, .. } = submit_public_claim_call(
        PublicClaimCallRequest {
            scope: &scope,
            request: &id,
            caller: &caller,
        },
        &mut port,
        PublicClaimCallLimits::default(),
    )?
    else {
        return Err("original unknown context".into());
    };
    let prepared = prepare_public_claim_request(
        original.frame(),
        CodecLimits::default(),
        ClaimCallerLimits::default(),
        32 * 1_048_576,
    )?;
    assert_eq!(prepared.prepared().funding_batch(), Some([0x7a; 16]));
    assert_eq!(prepared.prepared().intent(), original.intent());
    assert_eq!(original.intent().version(), 21);
    drop(prepared);
    let pointer = original.frame().as_ptr();
    let PublicClaimCallResult::Unknown { original, .. } =
        retry_public_claim_call(original, &mut port, PublicClaimCallLimits::default())?
    else {
        return Err("still unknown".into());
    };
    assert_eq!(original.frame().as_ptr(), pointer);
    assert_eq!(
        port.trace,
        [
            ClaimTrace::Lookup,
            ClaimTrace::Prepare,
            ClaimTrace::Submit,
            ClaimTrace::Lookup,
            ClaimTrace::Submit
        ]
    );
    assert!(Arc::ptr_eq(&port.submitted[0], &port.submitted[1]));
    Ok(())
}
#[test]
fn refused_retry_keeps_original_and_a_later_original_lookup_never_resubmits() -> TestResult {
    let (scope, id, caller) = (scope()?, id()?, caller()?);
    let mut port = ClaimSubmitFixture::new(ClaimPortMode::Fresh);
    let PublicClaimCallResult::Unknown { original, .. } = submit_public_claim_call(
        PublicClaimCallRequest {
            scope: &scope,
            request: &id,
            caller: &caller,
        },
        &mut port,
        PublicClaimCallLimits::default(),
    )?
    else {
        return Err("unknown".into());
    };
    let pointer = original.frame().as_ptr();
    let limits = PublicClaimCallLimits {
        workspace_bytes: 1,
        ..PublicClaimCallLimits::default()
    };
    let refusal = retry_public_claim_call(original, &mut port, limits)
        .err()
        .ok_or("current budget refusal")?;
    let original = refusal.into_original();
    assert_eq!(original.frame().as_ptr(), pointer);
    assert_eq!(port.trace.len(), 3);
    port.mode = ClaimPortMode::Retained;
    port.retained = Some(outcome(&scope, &id, &caller)?);
    assert!(matches!(
        retry_public_claim_call(original, &mut port, PublicClaimCallLimits::default())?,
        PublicClaimCallResult::Retained(_)
    ));
    assert_eq!(
        port.trace,
        [
            ClaimTrace::Lookup,
            ClaimTrace::Prepare,
            ClaimTrace::Submit,
            ClaimTrace::Lookup
        ]
    );
    Ok(())
}
