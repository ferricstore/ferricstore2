//! One actual original GET read and a callback borrowing its paid raw owner.

use super::{OriginalPolicyTransactionError as Error, RawError, workspace};
use crate::state::{RedbStateStore, RedbStateTransaction};
use crate::transaction::RecordBatchTransactionLimits;
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, CompletePolicyValue, OriginalPolicyGetRejection,
    OriginalPolicyGetRequest, OriginalPolicyReadValue, OriginalPolicyReply,
    OriginalPolicyReplyContext, OriginalPolicyReplyError, OriginalPolicyReplyRejection,
    PolicySchemaCharge, PolicySchemaError, RawFlowPolicyValue, StoredPolicyValue,
    encode_original_policy_from_complete, original_policy_from_complete_bytes,
    original_policy_get_reply_projection_bytes, original_policy_projection_reply_bytes,
    prepare_original_policy_get_request, present_original_policy_get_reply,
    present_original_policy_projection_get_reply,
};
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, GroupCorePolicyTransaction,
    GroupOriginalPolicyTransaction, StateTransactionError,
};

/// Prepares original GET controls before I/O and visits one actual paid reply.
///
/// The raw owner and writer guard survive the visit. The visitor receives the
/// same schema account; its own CPU and escaped owned copies are external work.
/// # Errors
/// Preserves physical caps, allocation and profile origins outside caller input
/// diagnoses, and preserves reached source exceptions from reply observation.
pub fn with_original_policy_get_reply(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    workflow_type: &RawFlowPolicyValue<'_>,
    opts: &RawFlowPolicyValue<'_>,
    limits: RecordBatchTransactionLimits,
    charge: &mut PolicySchemaCharge,
    visit: impl for<'reply> FnOnce(
        Result<OriginalPolicyReply<'reply>, OriginalPolicyGetRejection>,
        &mut PolicySchemaCharge,
    ) -> Result<(), OriginalPolicyReplyError>,
) -> Result<(), Error> {
    if charge.profile() != limits.schema {
        return Err(CompletePolicyEncodingError::Schema(PolicySchemaError::ProfileMismatch).into());
    }
    // Front key allocation belongs to the admitted Pg stage, not an invented
    // physical S debit. Pure operational caps precede local input diagnosis.
    admit_workspace(fixed(limits)?, workspace(limits)?, limits)?;
    let request =
        match prepare_original_policy_get_request(workflow_type, opts, limits.schema, charge)? {
            Ok(request) => request,
            Err(reason) => return visit(Err(reason), charge).map_err(Error::Reply),
        };
    read_reply(store, scope, request, limits, charge, visit)
}

fn read_reply(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    request: OriginalPolicyGetRequest<'_>,
    limits: RecordBatchTransactionLimits,
    charge: &mut PolicySchemaCharge,
    visit: impl for<'reply> FnOnce(
        Result<OriginalPolicyReply<'reply>, OriginalPolicyGetRejection>,
        &mut PolicySchemaCharge,
    ) -> Result<(), OriginalPolicyReplyError>,
) -> Result<(), Error> {
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        limits.codec.admission(),
        limits.maximum_read_steps,
        limits.maximum_read_bytes,
        limits.maximum_adapter_bytes,
    )?;
    let ceiling = tx.core_policy_schema_limits();
    if charge.profile() != limits.schema
        || limits.schema.policy_limits().max_states() > ceiling.policy_limits().max_states()
        || limits.schema.max_work_bytes() > ceiling.max_work_bytes()
    {
        return Err(StateTransactionError::InvalidChanges.into());
    }
    if tx.original_policy_profile()? != 1 {
        return Err(StateTransactionError::UnsupportedPolicyProfile.into());
    }
    let kind = request.workflow_type();
    let state = request.response_state();
    drop(request);
    let raw = match tx.stored_policy_value(scope.tenant(), kind, limits.schema, charge)? {
        Some(StoredPolicyValue::Complete(value)) => {
            return projected_reply(&tx, kind, state, value, limits, charge, visit);
        }
        Some(StoredPolicyValue::Original(raw)) => Some(raw),
        None => None,
    };
    preflight(&tx, raw.as_deref(), state, None, limits, charge)?;
    let defaults = tx.original_policy_reply_defaults();
    let context =
        OriginalPolicyReplyContext::portable(defaults, if raw.is_some() { 512 } else { 0 });
    let value = raw.as_deref().map_or(
        OriginalPolicyReadValue::Absent,
        OriginalPolicyReadValue::Binary,
    );
    let reply = public_reply(present_original_policy_get_reply(
        kind,
        value,
        state,
        context,
        limits.schema,
        charge,
    )?)?;
    visit(reply, charge).map_err(Error::Reply)
}

fn projected_reply(
    tx: &RedbStateTransaction<'_>,
    kind: &[u8],
    state: Option<&[u8]>,
    value: Box<CompletePolicyValue>,
    limits: RecordBatchTransactionLimits,
    charge: &mut PolicySchemaCharge,
    visit: impl for<'reply> FnOnce(
        Result<OriginalPolicyReply<'reply>, OriginalPolicyGetRejection>,
        &mut PolicySchemaCharge,
    ) -> Result<(), OriginalPolicyReplyError>,
) -> Result<(), Error> {
    let (length, _) = original_policy_from_complete_bytes(&value, limits.schema, charge, 0)?;
    let conversion = length
        .checked_add(512)
        .ok_or(StateTransactionError::Limit)?;
    super::admit_staging(conversion, limits)?;
    let encoded = encode_original_policy_from_complete(&value, limits.schema, charge, 0, length)?;
    drop(value);
    preflight(tx, Some(&encoded), state, Some(length), limits, charge)?;
    let context = OriginalPolicyReplyContext::portable(tx.original_policy_reply_defaults(), 512);
    let reply = public_reply(present_original_policy_projection_get_reply(
        kind,
        &encoded,
        state,
        context,
        limits.schema,
        charge,
        length,
    )?)?;
    visit(reply, charge).map_err(Error::Reply)
}

fn public_reply(
    reply: Result<OriginalPolicyReply<'_>, OriginalPolicyReplyRejection>,
) -> Result<Result<OriginalPolicyReply<'_>, OriginalPolicyGetRejection>, Error> {
    match reply {
        Ok(reply) => Ok(Ok(reply)),
        Err(OriginalPolicyReplyRejection::Corrupt) => Ok(Err(OriginalPolicyGetRejection::Corrupt)),
        Err(OriginalPolicyReplyRejection::ShardUnavailable) => {
            Ok(Err(OriginalPolicyGetRejection::ShardUnavailable))
        }
        Err(OriginalPolicyReplyRejection::ReadFailed) => {
            Ok(Err(OriginalPolicyGetRejection::ReadFailed))
        }
        Err(OriginalPolicyReplyRejection::AllocationFailed) => {
            Err(CompletePolicyEncodingError::Malformed.into())
        }
    }
}

fn preflight(
    tx: &RedbStateTransaction<'_>,
    raw: Option<&[u8]>,
    state: Option<&[u8]>,
    projection_limit: Option<usize>,
    limits: RecordBatchTransactionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let validation = raw.map_or(Ok(0), super::validation)?;
    // This measurement is a real extra decode/catalog pass, admitted in the
    // same cumulative C account before its index allocation.
    tx.original_policy_temporary(validation)?;
    let reply_bytes = match (raw, projection_limit) {
        (Some(raw), Some(maximum)) => {
            original_policy_projection_reply_bytes(raw, state, maximum, limits.schema, charge)?
        }
        _ => original_policy_get_reply_projection_bytes(raw, state, limits.schema, charge)?,
    };
    let raw_owner = raw.map_or(Ok(0), |value| {
        value
            .len()
            .checked_add(512)
            .ok_or(StateTransactionError::Limit)
    })?;
    let required = fixed(limits)?
        .checked_add(raw_owner)
        .and_then(|n| n.checked_add(reply_bytes))
        .ok_or(StateTransactionError::Limit)?;
    admit_workspace(required, workspace(limits)?, limits)?;
    // The actual presenter decodes again; the measurement grants no index or
    // payment reuse. Its separate X prepayment precedes that second allocation.
    tx.original_policy_temporary(validation)?;
    Ok(())
}

fn fixed(limits: RecordBatchTransactionLimits) -> Result<usize, StateTransactionError> {
    usize::try_from(limits.schema.max_work_bytes())
        .map_err(|_| StateTransactionError::Limit)?
        .checked_add(65_536)
        .ok_or(StateTransactionError::Limit)
}

fn admit_workspace(
    required: usize,
    available: usize,
    limits: RecordBatchTransactionLimits,
) -> Result<(), Error> {
    if required > available {
        return Err(super::original_error(
            RawError::Workspace {
                bytes: required,
                maximum: available,
            },
            limits,
        ));
    }
    Ok(())
}
