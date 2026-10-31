//! Original-first public Claim orchestration over an authorized supervised port.
mod dispatch;
mod model;
mod prepare;
pub use model::{
    PreparedPublicClaimCall, PublicClaimCallError, PublicClaimCallLimits, PublicClaimCallRequest,
    PublicClaimCallResult, PublicClaimRetryError,
};
use wf_ports::consensus::PublicClaimSubmitPort;
/// Resolves the original request before trusted time/nonce or current funding.
/// # Errors
/// Refuses before any new proposal through this entrypoint.
pub fn submit_public_claim_call<P: PublicClaimSubmitPort>(
    request: PublicClaimCallRequest<'_>,
    port: &mut P,
    limits: PublicClaimCallLimits,
) -> Result<PublicClaimCallResult, PublicClaimCallError> {
    prepare::preflight(request.scope, request.request, limits)?;
    let v1 = wf_ports::log::prepare_public_claim_intent(
        request.scope.tenant(),
        request.caller,
        limits.maximum_intent_bytes,
    )
    .map_err(PublicClaimCallError::Codec)?;
    let intent = wf_domain::workflow::request::ClientIntent::public_claim(*v1.digest());
    if let wf_ports::consensus::PublicClaimRequestLookup::Resolved(result) = port
        .lookup_public_claim(request.scope, request.request, intent, limits.read)
        .map_err(PublicClaimCallError::Preparation)?
    {
        return Ok(dispatch::lookup(result, request, intent, limits));
    }
    let original = prepare::original(request, port, intent, limits)?;
    dispatch::submit(original, port, limits, false).map_err(|error| error.error)
}
/// Looks up and, if unresolved but absent, resubmits the same immutable original.
/// # Errors
/// Retains the original on current preparation/resource admission failure.
pub fn retry_public_claim_call<P: PublicClaimSubmitPort>(
    original: Box<PreparedPublicClaimCall>,
    port: &mut P,
    limits: PublicClaimCallLimits,
) -> Result<PublicClaimCallResult, PublicClaimRetryError> {
    if let Err(error) = prepare::preflight(&original.scope, &original.request, limits) {
        return Err(PublicClaimRetryError { error, original });
    }
    dispatch::submit(original, port, limits, true)
}
