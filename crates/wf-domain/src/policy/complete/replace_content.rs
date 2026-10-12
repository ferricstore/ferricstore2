//! Borrowed source-ordered Replace content checks and independent response work.

use crate::retention::MAX_HISTORY_EVENTS;

use super::admission::{charge_schema, complete_metrics};
use super::{CompletePolicyData, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits};

mod core;
mod governance;
mod indexed;
mod rejection;

pub use rejection::{
    CompletePolicyReplaceRejection, CompletePolicyReplaceRejectionKind,
    encode_complete_policy_replace_rejection_body,
};

/// Checks representable normalized caller fields once in source business order.
///
/// Operational metrics precede business checks. Success does not grant Fresh,
/// installation or reusable payment authority, and leaves all owners unchanged.
/// Multiple bad map entries follow the typed raw-byte map order; general Elixir
/// map first-error equivalence and raw normalization remain outer obligations.
///
/// # Errors
/// Preserves profile, captured-context, cap, correspondence and work refusals as
/// outer errors. Only reached source-content failures return a private rejection.
pub fn check_complete_policy_replace_content(
    data: &CompletePolicyData,
    history_max_events_limit: u32,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<CompletePolicyReplaceRejection>, PolicySchemaError> {
    if charge.profile() != limits {
        return Err(PolicySchemaError::ProfileMismatch);
    }
    charge_schema(charge, 128, 0)?;
    if history_max_events_limit == 0 || history_max_events_limit > MAX_HISTORY_EVENTS {
        return Err(PolicySchemaError::InvalidAuthority);
    }
    let (rows, bytes, largest) = complete_metrics(data, limits, charge)?;
    let work = rows
        .checked_mul(128)
        .and_then(|n| n.checked_add(bytes))
        .ok_or(PolicySchemaError::LengthOverflow)?;
    charge_schema(charge, work, 0)?;
    let scratch = largest
        .checked_mul(64)
        .ok_or(PolicySchemaError::LengthOverflow)?;
    charge_schema(charge, 0, scratch)?;
    if let Some(kind) = self::core::check_replace_core(data, history_max_events_limit)? {
        return rejection::reject(kind).map(Some);
    }
    if let Some(kind) = indexed::check_replace_indexed(data, charge)? {
        return rejection::reject(kind).map(Some);
    }
    if let Some(group) = &data.governance
        && let Some(kind) = governance::check_replace_governance(group, charge)?
    {
        return rejection::reject(kind).map(Some);
    }
    charge_schema(charge, 128, 0)?;
    for (name, state) in &data.states {
        charge_visit(charge, name.len())?;
        if let Some(kind) = self::core::check_replace_state(name, state, history_max_events_limit)?
        {
            return rejection::reject(kind).map(Some);
        }
        if let Some(group) = &state.governance
            && let Some(kind) = governance::check_replace_governance(group, charge)?
        {
            return rejection::reject(kind).map(Some);
        }
    }
    Ok(None)
}

fn charge_visit(charge: &mut PolicySchemaCharge, length: usize) -> Result<(), PolicySchemaError> {
    let work = u64::try_from(length)
        .ok()
        .and_then(|n| n.checked_add(128))
        .ok_or(PolicySchemaError::LengthOverflow)?;
    charge_schema(charge, work, 0)
}

pub(super) fn complete_policy_replace_rejection_body_len(
    rejection: CompletePolicyReplaceRejection,
) -> Result<usize, PolicySchemaError> {
    rejection::body_len(rejection)
}
