//! Whole-call admission precedes lookup; generated inputs belong to one frame.
use super::{
    PreparedPublicClaimCall, PublicClaimCallError as E, PublicClaimCallLimits as Limits,
    PublicClaimCallRequest as Request,
};
use std::sync::Arc;
use wf_domain::workflow::request::{ClientIntent, RequestScope};
use wf_ports::{
    consensus::PublicClaimSubmitPort,
    log::{PublicClaimPreparation, encode_public_claim_request},
};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};

pub(super) fn preflight(scope: &RequestScope, id: &RequestId, limits: Limits) -> Result<(), E> {
    if [
        limits.read.maximum_steps,
        limits.read.maximum_bytes,
        limits.read.maximum_temporary_bytes,
        limits.maximum_intent_bytes,
        limits.maximum_prepared_bytes,
        limits.maximum_response_bytes,
        limits.execution_workspace_bytes,
        limits.workspace_bytes,
    ]
    .contains(&0)
        || limits.token_version != 1
        || limits.scan_budget > 16_384
        || limits.head_budget > 4096
        || !(1..=1_u64 << 32).contains(&limits.reply.credit_shard_count)
        || limits.reply.admission != limits.read.codec.admission()
    {
        return Err(E::InvalidOptions);
    }
    let namespace = namespace_bytes(scope, id)?;
    let frames = limits
        .maximum_prepared_bytes
        .checked_mul(2)
        .ok_or(E::Limit)?;
    let scratch = limits
        .maximum_response_bytes
        .checked_mul(4)
        .and_then(|n| n.checked_add(65_536))
        .ok_or(E::Limit)?;
    let total = [
        namespace,
        frames,
        scratch,
        limits.maximum_response_bytes,
        limits.read.maximum_bytes,
        limits.read.maximum_temporary_bytes,
        limits.execution_workspace_bytes,
    ]
    .into_iter()
    .try_fold(0_usize, usize::checked_add)
    .ok_or(E::Limit)?;
    if total > limits.workspace_bytes {
        return Err(E::Limit);
    }
    Ok(())
}
pub(super) fn namespace_bytes(scope: &RequestScope, id: &RequestId) -> Result<usize, E> {
    [
        scope.tenant().as_bytes(),
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        id.as_bytes(),
    ]
    .into_iter()
    .try_fold(4096_usize, |n, b| n.checked_add(b.len()))
    .ok_or(E::Limit)
}
pub(super) fn original<P: PublicClaimSubmitPort>(
    request: Request<'_>,
    port: &mut P,
    intent: ClientIntent,
    limits: Limits,
) -> Result<Box<PreparedPublicClaimCall>, E> {
    let (stamp, batch) = port
        .prepare_public_claim(request.scope, request.request, request.caller)
        .map_err(E::Preparation)?;
    let frame = encode_public_claim_request(
        request.scope,
        request.request,
        stamp,
        request.caller,
        PublicClaimPreparation {
            token_version: limits.token_version,
            first_ordinal: limits.first_ordinal,
            scan_budget: limits.scan_budget,
            head_budget: limits.head_budget,
            batch,
        },
        limits.read.codec,
        limits.maximum_prepared_bytes,
    )
    .map_err(E::Codec)?;
    Ok(Box::new(PreparedPublicClaimCall {
        scope: copy_scope(request.scope)?,
        request: RequestId::new(copy(request.request.as_bytes())?)
            .map_err(|_| E::InvalidOptions)?,
        intent,
        frame: Arc::new(frame),
    }))
}
fn copy_scope(scope: &RequestScope) -> Result<RequestScope, E> {
    Ok(RequestScope::new(
        TenantId::new(copy(scope.tenant().as_bytes())?).map_err(|_| E::InvalidOptions)?,
        GroupId::new(copy(scope.group().as_bytes())?).map_err(|_| E::InvalidOptions)?,
        RecoveryId::new(copy(scope.recovery().as_bytes())?).map_err(|_| E::InvalidOptions)?,
    ))
}
fn copy(bytes: &[u8]) -> Result<Vec<u8>, E> {
    let mut out = Vec::new();
    out.try_reserve_exact(bytes.len())
        .map_err(|_| E::Allocation)?;
    out.extend_from_slice(bytes);
    Ok(out)
}
