//! Public credit caller sequencing; every committed mutation revalidates authority.
mod dispatch;
mod model;
mod prepare;
pub use model::{
    PreparedPublicCreditCall, PublicCreditCallError, PublicCreditCallLimits,
    PublicCreditCallRequest, PublicCreditCallResult, PublicCreditRetryError,
};
use wf_domain::governance::caller::CreditCallAction as A;
use wf_ports::consensus::PublicCreditSubmitPort;
use wf_ports::{
    consensus::CreditRequestLookup, log::public_credit_intent, state::StateTransactionError,
};
/// Resolves originals, repairs existing membership, prepares and submits one caller.
/// # Errors
/// Refuses before CREDIT proposal; catalog repairs may already have committed.
pub fn submit_public_credit_call<P: PublicCreditSubmitPort>(
    request: PublicCreditCallRequest<'_>,
    port: &mut P,
    limits: PublicCreditCallLimits,
) -> Result<PublicCreditCallResult, PublicCreditCallError> {
    use PublicCreditCallError as E;
    prepare::preflight(
        request.source,
        request.target,
        request.catalog,
        request.caller.scope(),
        request.request,
        limits,
    )?;
    let intent = public_credit_intent(request.caller).map_err(E::Codec)?;
    if let CreditRequestLookup::Resolved(result) = port
        .lookup_public_credit(
            request.source,
            request.request,
            intent,
            limits.publication.read,
        )
        .map_err(E::Preparation)?
    {
        return Ok(dispatch::lookup(result, request, intent, limits));
    }
    let owner = port
        .credit_catalog_owner(
            request.source,
            request.caller.scope(),
            limits.publication.read,
        )
        .map_err(E::Read)?;
    if owner
        .as_ref()
        .is_some_and(|owner| owner.view().scope != request.caller.scope())
    {
        return Err(E::Read(StateTransactionError::Corrupt));
    }
    let exists = owner.is_some();
    let epoch = prepare::operands(request.caller)
        .0
        .and_then(|shard| owner.as_ref().and_then(|owner| owner.lease(shard)))
        .map_or(0, |lease| lease.epoch);
    drop(owner);
    if exists {
        super::repair::repair_existing_credit_catalog(
            port,
            request.source,
            request.target,
            request.catalog,
            request.caller.scope(),
            limits.publication,
        )
        .map_err(E::Repair)?;
    } else {
        match request.caller.action() {
            A::Spend { .. } => return Err(E::NotFound),
            A::Lease {
                configuration: None,
                ..
            } => return Err(E::ConfigurationRequired),
            A::Get => return Ok(PublicCreditCallResult::Missing),
            A::Cleanup => return Ok(PublicCreditCallResult::EmptyCleanup),
            _ => (),
        }
    }
    let original = prepare::original(request, port, intent, epoch, limits)?;
    dispatch::submit(original, port, limits).map_err(|error| error.error)
}
/// Re-admits and submits the exact original frame without a fresh read or spend.
/// # Errors
/// Refuses current budgets before resubmission, preserving original ownership.
pub fn retry_public_credit_call<P: PublicCreditSubmitPort>(
    original: Box<PreparedPublicCreditCall>,
    port: &mut P,
    limits: PublicCreditCallLimits,
) -> Result<PublicCreditCallResult, PublicCreditRetryError> {
    if let Err(error) = prepare::preflight(
        &original.source,
        &original.target,
        &original.catalog,
        &original.credit_scope,
        &original.request,
        limits,
    ) {
        return Err(PublicCreditRetryError { error, original });
    }
    if original
        .frame()
        .len()
        .checked_add(4096)
        .is_none_or(|n| n > limits.maximum_prepared_bytes)
        || original
            .ids
            .iter()
            .try_fold(2048_usize, |n, id| {
                n.checked_add(id.len() + std::mem::size_of::<Vec<u8>>())
            })
            .is_none_or(|n| n > limits.maximum_ids_bytes)
    {
        return Err(PublicCreditRetryError {
            error: PublicCreditCallError::Limit,
            original,
        });
    }
    dispatch::submit(original, port, limits)
}
