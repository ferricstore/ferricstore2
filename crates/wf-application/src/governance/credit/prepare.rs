//! Prospective whole-call admission and original immutable preparation ownership.
use super::{
    PreparedPublicCreditCall, PublicCreditCallError as E, PublicCreditCallLimits as Limits,
    PublicCreditCallRequest as Request,
};
use std::sync::Arc;
use wf_domain::{
    governance::caller::{CreditCallAction as A, NormalizedCreditCall},
    workflow::request::{ClientIntent, RequestScope},
};
use wf_ports::{
    consensus::PublicCreditSubmitPort,
    log::{CreditSpendPreparation, encode_public_credit_request, generate_public_credit_ids},
};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};

pub(super) fn preflight(
    source: &RequestScope,
    target: &RequestScope,
    catalog: &[u8],
    credit_scope: &[u8],
    request: &RequestId,
    limits: Limits,
) -> Result<(), E> {
    super::super::publication::plan::select(
        super::super::CreditCatalogPublicationRequest {
            sources: std::slice::from_ref(source),
            source_cursor: 0,
            target,
            catalog,
        },
        limits.publication,
    )
    .map_err(|error| match error {
        super::super::CreditCatalogPublicationError::InvalidOptions => E::InvalidOptions,
        super::super::CreditCatalogPublicationError::Limit => E::Limit,
        super::super::CreditCatalogPublicationError::Allocation => E::Allocation,
    })?;
    if credit_scope.is_empty()
        || credit_scope.len() > 65535
        || [
            limits.maximum_prepared_bytes,
            limits.maximum_ids_bytes,
            limits.maximum_response_bytes,
            limits.workspace_bytes,
        ]
        .contains(&0)
    {
        return Err(E::InvalidOptions);
    }
    let sizes = [
        source.tenant().as_bytes(),
        source.group().as_bytes(),
        source.recovery().as_bytes(),
        target.tenant().as_bytes(),
        target.group().as_bytes(),
        target.recovery().as_bytes(),
        catalog,
        credit_scope,
        request.as_bytes(),
    ];
    let namespace = sizes
        .into_iter()
        .try_fold(4096_usize, |n, bytes| n.checked_add(bytes.len()))
        .ok_or(E::Limit)?;
    let total = [
        namespace,
        limits.publication.workspace_bytes,
        limits.maximum_prepared_bytes,
        limits.maximum_ids_bytes,
        limits.maximum_response_bytes,
        18432,
    ]
    .into_iter()
    .try_fold(0_usize, usize::checked_add)
    .ok_or(E::Limit)?;
    if total > limits.workspace_bytes {
        return Err(E::Limit);
    }
    Ok(())
}

pub(super) fn operands(call: NormalizedCreditCall<'_>) -> (Option<u64>, Option<u64>) {
    match call.action() {
        A::Lease {
            shard_id, amount, ..
        }
        | A::Spend {
            shard_id, amount, ..
        }
        | A::Release {
            shard_id, amount, ..
        } => (Some(shard_id), Some(amount)),
        A::Renew { shard_id, .. } => (Some(shard_id), None),
        A::Get | A::Cleanup => (None, None),
    }
}

pub(super) fn original<P: PublicCreditSubmitPort>(
    request: Request<'_>,
    port: &mut P,
    intent: ClientIntent,
    epoch: u64,
    limits: Limits,
) -> Result<Box<PreparedPublicCreditCall>, E> {
    let (stamp, batch) = port
        .prepare_public_credit(request.source, request.request, request.caller)
        .map_err(E::Preparation)?;
    let (shard, amount) = operands(request.caller);
    let spend = match (request.caller.action(), batch) {
        (A::Spend { .. }, Some(batch)) => Some(CreditSpendPreparation {
            lease_epoch: epoch,
            batch,
        }),
        (A::Spend { .. }, None) | (_, Some(_)) => return Err(E::InvalidOptions),
        (_, None) => None,
    };
    let ids = match spend {
        Some(spend) => generate_public_credit_ids(
            amount.ok_or(E::InvalidOptions)?,
            spend,
            limits.maximum_ids_bytes,
        )
        .map_err(E::Codec)?,
        None => Vec::new(),
    };
    let frame = encode_public_credit_request(
        request.source,
        request.request,
        request.caller,
        stamp,
        spend,
        limits.publication.codec,
        limits.maximum_prepared_bytes,
    )
    .map_err(E::Codec)?;
    Ok(Box::new(PreparedPublicCreditCall {
        source: copy_scope(request.source)?,
        target: copy_scope(request.target)?,
        request: RequestId::new(copy(request.request.as_bytes())?)
            .map_err(|_| E::InvalidOptions)?,
        catalog: copy(request.catalog)?,
        credit_scope: copy(request.caller.scope())?,
        kind: request.caller.action().kind(),
        shard,
        amount,
        intent,
        minimum_time: request.caller.now_ms(),
        ids,
        frame: Arc::new(frame),
    }))
}

fn copy(bytes: &[u8]) -> Result<Vec<u8>, E> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes.len())
        .map_err(|_| E::Allocation)?;
    output.extend_from_slice(bytes);
    Ok(output)
}
fn copy_scope(scope: &RequestScope) -> Result<RequestScope, E> {
    Ok(RequestScope::new(
        TenantId::new(copy(scope.tenant().as_bytes())?).map_err(|_| E::InvalidOptions)?,
        GroupId::new(copy(scope.group().as_bytes())?).map_err(|_| E::InvalidOptions)?,
        RecoveryId::new(copy(scope.recovery().as_bytes())?).map_err(|_| E::InvalidOptions)?,
    ))
}
