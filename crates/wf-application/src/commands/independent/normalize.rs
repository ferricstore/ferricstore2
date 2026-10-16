//! Whole-call raw/resource admission and borrowed caller normalization.

use wf_domain::workflow::admission::MAX_BATCH_ITEMS;
use wf_domain::workflow::caller::{
    MutationCallerError, MutationFieldValue, NormalizedMutation, measure_core_mutation_input,
    normalize_core_mutation,
};
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::{CodecError, RecordBatchApplyError, measure_public_mutation};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};

use super::{
    IndependentMutationCall, MutationCallLimits, MutationItemInput, MutationItemResult,
    MutationItemStatus,
};

pub(super) fn prepare<'a>(
    call: IndependentMutationCall<'a>,
    limits: MutationCallLimits,
    callers: &mut Vec<Result<NormalizedMutation<'a>, MutationCallerError>>,
    results: &mut Vec<MutationItemResult>,
) -> Result<usize, MutationCallerError> {
    validate_limits(limits)?;
    match call {
        IndependentMutationCall::TerminalMany {
            operation,
            base_partition,
            common,
            items,
            ..
        } => {
            if items.is_empty() {
                return Err(MutationCallerError::EmptyMany);
            }
            prepare_items(
                items,
                common,
                base_partition,
                Some(operation),
                limits,
                callers,
                results,
            )
        }
        IndependentMutationCall::TerminalPipeline { items } => {
            prepare_items(items, &[], None, None, limits, callers, results)
        }
    }
}

fn validate_limits(limits: MutationCallLimits) -> Result<(), MutationCallerError> {
    if limits.maximum_items == 0
        || limits.maximum_items > MAX_BATCH_ITEMS
        || limits.maximum_raw_bytes == 0
        || limits.maximum_result_bytes == 0
        || limits.execution_workspace_bytes == 0
        || limits.workspace_bytes == 0
    {
        return Err(MutationCallerError::InvalidLimits);
    }
    Ok(())
}

fn prepare_items<'a>(
    items: &'a [MutationItemInput<'a>],
    common: &'a [(&'a str, MutationFieldValue<'a>)],
    base: Option<&'a [u8]>,
    operation: Option<&str>,
    limits: MutationCallLimits,
    callers: &mut Vec<Result<NormalizedMutation<'a>, MutationCallerError>>,
    results: &mut Vec<MutationItemResult>,
) -> Result<usize, MutationCallerError> {
    let global = preflight(items, common, base, operation, limits)?;
    callers
        .try_reserve_exact(items.len())
        .map_err(|_| MutationCallerError::Allocation)?;
    results
        .try_reserve_exact(items.len())
        .map_err(|_| MutationCallerError::Allocation)?;
    for item in items {
        let caller = match normalize_item(item, common, base, operation, limits) {
            Ok(caller) => {
                match measure_public_mutation(item.scope, item.request, &caller, limits.codec) {
                    Ok(encoded) => {
                        check(sum(&[global, 2_048, encoded])?, limits.workspace_bytes)?;
                        Ok(caller)
                    }
                    Err(error) => return Err(measure_error(error)),
                }
            }
            Err(error) => Err(error),
        };
        if operation.is_some() {
            caller?;
        }
        let status = match caller {
            Ok(_) => MutationItemStatus::NotSubmitted(
                wf_ports::consensus::MutationNotSubmitted::Preparation,
            ),
            Err(error) => MutationItemStatus::LocalError(error),
        };
        results.push(MutationItemResult {
            scope: copy_scope(item.scope)?,
            request: RequestId::new(copy(item.request.as_bytes())?)
                .map_err(|_| MutationCallerError::InvalidId)?,
            intent: None,
            status,
        });
        callers.push(caller);
    }
    Ok(global)
}

fn normalize_item<'a>(
    item: &MutationItemInput<'a>,
    common: &'a [(&'a str, MutationFieldValue<'a>)],
    base: Option<&'a [u8]>,
    operation: Option<&str>,
    limits: MutationCallLimits,
) -> Result<NormalizedMutation<'a>, MutationCallerError> {
    if let Some(operation) = operation {
        terminal(operation)?;
        if item.input.operation != operation {
            return Err(MutationCallerError::InvalidField("operation"));
        }
    }
    terminal(item.input.operation)?;
    normalize_core_mutation(
        item.input,
        common,
        base,
        limits.codec.admission(),
        limits.maximum_raw_bytes,
    )
}

fn terminal(operation: &str) -> Result<(), MutationCallerError> {
    match operation {
        "complete" | "fail" | "retry" | "cancel" => Ok(()),
        "create" => Err(MutationCallerError::UnsupportedFeature("create")),
        "transition" => Err(MutationCallerError::UnsupportedFeature("transition")),
        "extend_lease" => Err(MutationCallerError::UnsupportedFeature("extend_lease")),
        _ => Err(MutationCallerError::InvalidField("operation")),
    }
}

fn preflight(
    items: &[MutationItemInput<'_>],
    common: &[(&str, MutationFieldValue<'_>)],
    base: Option<&[u8]>,
    operation: Option<&str>,
    limits: MutationCallLimits,
) -> Result<usize, MutationCallerError> {
    if items.len() > limits.maximum_items {
        return Err(MutationCallerError::ItemLimit);
    }
    let mut raw = if let Some(operation) = operation {
        sum(&[
            operation.len(),
            product(items.len(), sum(&[1, base.map_or(0, <[u8]>::len)])?)?,
        ])?
    } else {
        0
    };
    let mut identities = 0;
    for item in items {
        let bytes = sum(&[
            item.scope.tenant().as_bytes().len(),
            item.scope.group().as_bytes().len(),
            item.scope.recovery().as_bytes().len(),
            item.request.as_bytes().len(),
        ])?;
        identities = sum(&[identities, bytes])?;
        raw = sum(&[
            raw,
            bytes,
            measure_core_mutation_input(item.input, common, limits.maximum_raw_bytes)?,
        ])?;
        check(raw, limits.maximum_raw_bytes)?;
    }
    check(raw, limits.maximum_raw_bytes)?;
    let output = sum(&[product(items.len(), 512)?, identities])?;
    check(output, limits.maximum_result_bytes)?;
    let global = sum(&[product(items.len(), 1_024)?, identities])?;
    check(
        sum(&[global, limits.execution_workspace_bytes])?,
        limits.workspace_bytes,
    )?;
    Ok(global)
}

fn measure_error(error: RecordBatchApplyError) -> MutationCallerError {
    match error {
        RecordBatchApplyError::Encoding(CodecError::Allocation) => MutationCallerError::Allocation,
        RecordBatchApplyError::Encoding(CodecError::LengthOverflow) => {
            MutationCallerError::LengthOverflow
        }
        RecordBatchApplyError::Encoding(CodecError::ItemLimit) => MutationCallerError::ItemLimit,
        RecordBatchApplyError::Encoding(_) | RecordBatchApplyError::Workspace { .. } => {
            MutationCallerError::RawBytes
        }
        RecordBatchApplyError::Apply(_)
        | RecordBatchApplyError::Retained(_)
        | RecordBatchApplyError::IntentMismatch => MutationCallerError::InvalidField("prepared"),
    }
}

fn copy_scope(scope: &RequestScope) -> Result<RequestScope, MutationCallerError> {
    let tenant = TenantId::new(copy(scope.tenant().as_bytes())?)
        .map_err(|_| MutationCallerError::InvalidId)?;
    let group = GroupId::new(copy(scope.group().as_bytes())?)
        .map_err(|_| MutationCallerError::InvalidId)?;
    let recovery = RecoveryId::new(copy(scope.recovery().as_bytes())?)
        .map_err(|_| MutationCallerError::InvalidId)?;
    Ok(RequestScope::new(tenant, group, recovery))
}

fn copy(bytes: &[u8]) -> Result<Vec<u8>, MutationCallerError> {
    let mut owner = Vec::new();
    owner
        .try_reserve_exact(bytes.len())
        .map_err(|_| MutationCallerError::Allocation)?;
    owner.extend_from_slice(bytes);
    Ok(owner)
}

fn product(count: usize, bytes: usize) -> Result<usize, MutationCallerError> {
    count
        .checked_mul(bytes)
        .ok_or(MutationCallerError::LengthOverflow)
}

fn sum(values: &[usize]) -> Result<usize, MutationCallerError> {
    values.iter().try_fold(0_usize, |total, value| {
        total
            .checked_add(*value)
            .ok_or(MutationCallerError::LengthOverflow)
    })
}

fn check(bytes: usize, maximum: usize) -> Result<(), MutationCallerError> {
    if bytes > maximum {
        Err(MutationCallerError::RawBytes)
    } else {
        Ok(())
    }
}
