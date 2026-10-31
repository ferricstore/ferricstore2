use super::{fixtures::*, *};
#[test]
fn whole_call_workspace_and_overflow_refuse_before_any_effect() -> TestResult {
    let (scope, id, caller) = (scope()?, id()?, caller()?);
    for limits in [
        PublicClaimCallLimits {
            workspace_bytes: 1,
            ..PublicClaimCallLimits::default()
        },
        PublicClaimCallLimits {
            maximum_prepared_bytes: usize::MAX,
            ..PublicClaimCallLimits::default()
        },
    ] {
        let mut port = ClaimSubmitFixture::new(ClaimPortMode::Fresh);
        assert!(matches!(
            submit_public_claim_call(
                PublicClaimCallRequest {
                    scope: &scope,
                    request: &id,
                    caller: &caller
                },
                &mut port,
                limits
            ),
            Err(PublicClaimCallError::Limit)
        ));
        assert_eq!(port.trace.len(), 0);
    }
    Ok(())
}
