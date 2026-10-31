use super::{fixtures::*, *};
#[test]
fn retained_original_is_resolved_before_time_nonce_or_funding_and_requested_now_is_identity_only()
-> TestResult {
    let (scope, id, caller) = (scope()?, id()?, caller()?);
    let original = outcome(&scope, &id, &caller)?;
    let mut port = ClaimSubmitFixture::new(ClaimPortMode::Retained);
    port.retained = Some(original.clone());
    let result = submit_public_claim_call(
        PublicClaimCallRequest {
            scope: &scope,
            request: &id,
            caller: &caller,
        },
        &mut port,
        PublicClaimCallLimits::default(),
    )?;
    let PublicClaimCallResult::Retained(actual) = result else {
        return Err("actual original retained reply".into());
    };
    assert_eq!(actual, original);
    assert_eq!(port.trace, [ClaimTrace::Lookup]);
    assert_eq!(port.intent, Some(original.intent()));
    Ok(())
}
#[test]
fn unresolved_original_or_current_authorization_denial_never_prepares_or_submits() -> TestResult {
    let (scope, id, caller) = (scope()?, id()?, caller()?);
    let mut port = ClaimSubmitFixture::new(ClaimPortMode::Unknown);
    assert!(matches!(
        submit_public_claim_call(
            PublicClaimCallRequest {
                scope: &scope,
                request: &id,
                caller: &caller
            },
            &mut port,
            PublicClaimCallLimits::default()
        )?,
        PublicClaimCallResult::Unresolved(U::Unavailable)
    ));
    assert_eq!(port.trace, [ClaimTrace::Lookup]);
    let mut denied = ClaimSubmitFixture::new(ClaimPortMode::Denied);
    assert!(matches!(
        submit_public_claim_call(
            PublicClaimCallRequest {
                scope: &scope,
                request: &id,
                caller: &caller
            },
            &mut denied,
            PublicClaimCallLimits::default()
        ),
        Err(PublicClaimCallError::Preparation(N::Unauthorized))
    ));
    assert_eq!(denied.trace, [ClaimTrace::Lookup]);
    Ok(())
}
#[test]
fn foreign_retained_namespace_or_intent_is_unresolved_without_new_preparation() -> TestResult {
    let (scope, id, caller) = (scope()?, id()?, caller()?);
    let foreign = RequestId::new(b"other".to_vec())?;
    let mut port = ClaimSubmitFixture::new(ClaimPortMode::Retained);
    port.retained = Some(outcome(&scope, &foreign, &caller)?);
    assert!(matches!(
        submit_public_claim_call(
            PublicClaimCallRequest {
                scope: &scope,
                request: &id,
                caller: &caller
            },
            &mut port,
            PublicClaimCallLimits::default()
        )?,
        PublicClaimCallResult::Unresolved(U::InvalidReply)
    ));
    assert_eq!(port.trace, [ClaimTrace::Lookup]);
    Ok(())
}
