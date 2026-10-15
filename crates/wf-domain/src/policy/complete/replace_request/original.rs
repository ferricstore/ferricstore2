//! Original raw-current replacement, distinct from positive private V2 inputs.

use std::collections::BTreeMap;
use std::fmt;

use wf_types::version::{LogIdentity, TimestampMillis};

use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;
use crate::workflow::request::ClientIntent;
use crate::workflow::request::gate::GateView;

use super::super::admission::charge_schema;
use super::super::patch::check_complete_core_patch_guard;
use super::super::{
    CompletePolicyData, CompletePolicyEncodingError, CompletePolicyReplaceInput, OriginalEtfSafety,
    OriginalPolicyEntry, OriginalPolicyReadError, OriginalPolicyReadValue, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits, decode_original_complete_policy_entry,
};
use super::{
    CompletePolicyReplaceApplyError, CompletePolicyReplaceFresh, ReplacePrepared,
    prepare_replace_content_result, replace_policy_rejection, retention,
};

/// Supplied coherent mathematical raw reads; this view performs no physical I/O.
#[derive(Clone, Copy)]
pub struct OriginalCompletePolicyReplaceApplyView<'view, 'raw, 'context> {
    /// Original retained/control authority, independent of the raw input owners.
    pub gate: GateView<'view>,
    /// Exact snapshot revision required by later atomic comparison.
    pub revision: u64,
    /// Captured original read classes; missing keys mean actual absence.
    pub policies: &'raw BTreeMap<Vec<u8>, OriginalPolicyReadValue<'raw>>,
    /// Same-type active migration generations or their operational reader failure.
    pub active_migrations: &'raw BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    /// Same-type completed migration generations or their operational reader failure.
    pub completed_migrations: &'raw BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    /// Immutable contextual history limit or its operational reader failure.
    pub history_max_events_limit: Result<u32, ApplyError>,
    /// Captured original outcome window or its operational reader failure.
    pub outcome_retention_ms: Result<u64, ApplyError>,
    /// Captured original decoder tables; these attest no live runtime authority.
    pub safety: OriginalEtfSafety<'context>,
}

/// Operational refusal, never an incoming caller rejection or retained status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalCompletePolicyReplaceApplyError {
    /// Existing frame/gate/common preparation and retention refusal.
    Replace(CompletePolicyReplaceApplyError),
    /// Original current-reader or reached post-CAS type-binding refusal.
    Current(OriginalPolicyReadError),
}

impl fmt::Display for OriginalCompletePolicyReplaceApplyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Replace(error) => fmt::Display::fmt(error, formatter),
            Self::Current(OriginalPolicyReadError::Corrupt) => {
                formatter.write_str("ERR corrupt flow policy high-water")
            }
            Self::Current(error) => fmt::Display::fmt(error, formatter),
        }
    }
}
impl std::error::Error for OriginalCompletePolicyReplaceApplyError {}

/// Composes strict original current decode, CAS and replacement on one account.
///
/// This mathematical Fresh operation verifies no frame and runs no request gate.
/// It does not normalize stored arbitrary maps or install original ETF bytes.
/// Its owned result uses the existing private semantic V2 output encoding.
///
/// # Errors
/// Keeps current decoding/type failures operational and retains only actual new
/// content or generation decisions through the unchanged replacement tail.
pub fn prepare_original_complete_policy_replace_fresh(
    view: OriginalCompletePolicyReplaceApplyView<'_, '_, '_>,
    input: CompletePolicyReplaceInput,
    committed: (ClientIntent, LogIdentity, TimestampMillis),
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyReplaceFresh, OriginalCompletePolicyReplaceApplyError> {
    use OriginalCompletePolicyReplaceApplyError::Replace;

    if charge.profile() != limits {
        return Err(Replace(CompletePolicyReplaceApplyError::Request(
            PolicySchemaError::ProfileMismatch.into(),
        )));
    }
    charge_schema(charge, 128, 0)
        .map_err(|error| Replace(CompletePolicyReplaceApplyError::Request(error.into())))?;
    let (scope, request, _stamp, expected, data) = input.into_parts();
    let prepared = prepare_original_result(&view, data, expected, limits, charge)?;
    retention::retain_replace_outcome(
        scope,
        request,
        committed,
        prepared,
        view.outcome_retention_ms,
        (limits, admission),
        charge,
    )
    .map_err(Replace)
}

fn prepare_original_result(
    view: &OriginalCompletePolicyReplaceApplyView<'_, '_, '_>,
    data: CompletePolicyData,
    expected: Option<u64>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<ReplacePrepared, OriginalCompletePolicyReplaceApplyError> {
    use OriginalCompletePolicyReplaceApplyError::Replace;

    let current = read_original_current(view, &data.workflow_type, limits, charge)?;
    let stored = current.as_ref().map_or(0, OriginalPolicyEntry::generation);
    charge_schema(charge, 128, 0)
        .map_err(|error| Replace(CompletePolicyReplaceApplyError::Request(error.into())))?;
    if let Err(error) = check_complete_core_patch_guard(stored, expected) {
        return Ok(Err(replace_policy_rejection(error).map_err(Replace)?));
    }
    if let Some(current) = current {
        bind_original_current_type(&current, &data.workflow_type, charge)?;
    }
    prepare_replace_content_result(
        data,
        stored,
        (view.active_migrations, view.completed_migrations),
        view.history_max_events_limit,
        limits,
        charge,
    )
    .map_err(Replace)
}

fn read_original_current<'raw>(
    view: &OriginalCompletePolicyReplaceApplyView<'_, 'raw, '_>,
    requested: &[u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<OriginalPolicyEntry<'raw>>, OriginalCompletePolicyReplaceApplyError> {
    use OriginalCompletePolicyReplaceApplyError::Current;

    let Some(raw) = read_original_current_bytes(view, requested, charge)? else {
        return Ok(None);
    };
    decode_original_complete_policy_entry(raw, &view.safety, limits, charge)
        .map_err(|error| Current(OriginalPolicyReadError::Encoding(error)))?
        .map(Some)
        .ok_or(Current(OriginalPolicyReadError::Corrupt))
}

pub(super) fn read_original_current_bytes<'raw>(
    view: &OriginalCompletePolicyReplaceApplyView<'_, 'raw, '_>,
    requested: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<Option<&'raw [u8]>, OriginalCompletePolicyReplaceApplyError> {
    use OriginalCompletePolicyReplaceApplyError::{Current, Replace};

    let work = u64::try_from(requested.len())
        .ok()
        .and_then(|length| length.checked_add(128))
        .ok_or(Replace(CompletePolicyReplaceApplyError::Request(
            PolicySchemaError::LengthOverflow.into(),
        )))?;
    charge_schema(charge, work, 0)
        .map_err(|error| Replace(CompletePolicyReplaceApplyError::Request(error.into())))?;
    let raw = match view.policies.get(requested) {
        None | Some(OriginalPolicyReadValue::Absent) => return Ok(None),
        Some(OriginalPolicyReadValue::Other) => {
            return Err(Current(OriginalPolicyReadError::Corrupt));
        }
        Some(OriginalPolicyReadValue::Unavailable) => {
            return Err(Current(OriginalPolicyReadError::ShardUnavailable));
        }
        Some(OriginalPolicyReadValue::ReadFailed) => {
            return Err(Current(OriginalPolicyReadError::ReadFailed));
        }
        Some(OriginalPolicyReadValue::Binary(raw)) => *raw,
    };
    Ok(Some(raw))
}

pub(super) fn bind_original_current_type(
    current: &OriginalPolicyEntry<'_>,
    requested: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<(), OriginalCompletePolicyReplaceApplyError> {
    use OriginalCompletePolicyReplaceApplyError::Current;

    let length = requested
        .len()
        .checked_add(current.workflow_type().map_or(0, <[u8]>::len))
        .and_then(|length| u64::try_from(length).ok())
        .and_then(|length| length.checked_add(128))
        .ok_or(Current(OriginalPolicyReadError::Encoding(
            CompletePolicyEncodingError::Schema(PolicySchemaError::LengthOverflow),
        )))?;
    charge_schema(charge, length, 0).map_err(|error| {
        Current(OriginalPolicyReadError::Encoding(
            CompletePolicyEncodingError::Schema(error),
        ))
    })?;
    if current.workflow_type() != Some(requested) {
        return Err(Current(OriginalPolicyReadError::Corrupt));
    }
    Ok(())
}
