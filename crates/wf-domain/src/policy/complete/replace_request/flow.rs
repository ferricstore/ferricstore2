//! Source allocator order and direct original envelopes from one incoming walk.

use super::super::admission::charge_schema;
use super::super::flow_input::value::{PolicyInputCursor as Cursor, PolicyInputKind as Kind};
use super::super::flow_input::{
    emit_original_flow_policy_envelope, prepare_original_flow_policy_layout,
};
use super::super::original_entry::decode_original_flow_policy_current_entry;
use super::super::{
    CompletePolicyEncodingError as Error, FlowPolicyInputRejection,
    OriginalFlowPolicyReplaceFrameHeader, OriginalPolicyReadError, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits,
};
use super::original::{bind_original_current_type, read_original_current_bytes};
use super::{
    CompletePolicyReplaceApplyError, OriginalCompletePolicyReplaceApplyError as ApplyErrorType,
    OriginalCompletePolicyReplaceApplyView,
};
use crate::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use crate::retention::MAX_HISTORY_EVENTS;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;
use crate::workflow::request::{ClientIntent, PolicyRejection, RetainedOutcome};
use wf_types::version::{LogIdentity, TimestampMillis};

mod retention;

/// Owned proposed original installation; no physical publication is performed.
pub struct OriginalFlowPolicyInstall {
    policy_key: Vec<u8>,
    stored_generation: u64,
    generation: u64,
    encoded: Vec<u8>,
}

impl OriginalFlowPolicyInstall {
    pub(in crate::policy::complete) const fn new_proposed(
        policy_key: Vec<u8>,
        stored_generation: u64,
        generation: u64,
        encoded: Vec<u8>,
    ) -> Self {
        Self {
            policy_key,
            stored_generation,
            generation,
            encoded,
        }
    }

    /// Borrows the independently owned exact destination key.
    #[must_use]
    pub fn policy_key(&self) -> &[u8] {
        &self.policy_key
    }
    /// Reads the coherent predecessor for a later atomic comparison.
    #[must_use]
    pub const fn stored_generation(&self) -> u64 {
        self.stored_generation
    }
    /// Reads the newly allocated generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    /// Borrows original deterministic ETF envelope bytes.
    #[must_use]
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
}

/// Fresh proposed installation and its separately owned immutable response.
pub type OriginalFlowPolicyReplaceFresh = (Option<OriginalFlowPolicyInstall>, RetainedOutcome);

/// Reached lower-command/business diagnoses, distinct from operational errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalFlowPolicyReplaceRejection {
    /// The lower replacement patch is not a map.
    PatchShape,
    /// The destination does not have the exact nonempty policy suffix.
    PolicyKey,
    /// The source incoming normalization reached this precise diagnosis.
    Input(FlowPolicyInputRejection),
}

enum FlowReplaceRejected {
    Policy(PolicyRejection),
    Flow(OriginalFlowPolicyReplaceRejection),
}
type FlowReplacePrepared = Result<OriginalFlowPolicyInstall, FlowReplaceRejected>;

/// Prepares source Fresh work without verifying a frame or reading a request gate.
///
/// The closed ports parent supplies one verified header and committed gate result.
/// Only current scalars/type survive its strict reader; arbitrary stored maps are
/// never normalized as incoming business data.
/// # Errors
/// Preserves operational origins and retains only actual reached caller diagnoses.
pub fn prepare_original_flow_policy_replace_fresh(
    view: OriginalCompletePolicyReplaceApplyView<'_, '_, '_>,
    header: OriginalFlowPolicyReplaceFrameHeader<'_>,
    committed: (ClientIntent, LogIdentity, TimestampMillis),
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<OriginalFlowPolicyReplaceFresh, ApplyErrorType> {
    if charge.profile() != limits {
        return Err(schema(PolicySchemaError::ProfileMismatch));
    }
    pay(charge, 128)?;
    let (scope, request, key, expected, patch) = header.into_parts();
    let prepared = prepare(
        &view,
        key,
        Cursor::Frame(expected),
        Cursor::Frame(patch),
        limits,
        charge,
    )?;
    retention::retain(
        scope,
        request,
        committed,
        prepared,
        view.outcome_retention_ms,
        admission,
        charge,
    )
}

fn prepare(
    view: &OriginalCompletePolicyReplaceApplyView<'_, '_, '_>,
    key: &[u8],
    expected: Cursor<'_>,
    patch: Cursor<'_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<FlowReplacePrepared, ApplyErrorType> {
    pay(charge, 128)?;
    if patch.kind() != Kind::Map {
        return Ok(Err(FlowReplaceRejected::Flow(
            OriginalFlowPolicyReplaceRejection::PatchShape,
        )));
    }
    pay(charge, plus(128, key.len())?)?;
    let Some(requested) = key
        .strip_prefix(b"f:{f}:policy:")
        .filter(|suffix| !suffix.is_empty())
    else {
        return Ok(Err(FlowReplaceRejected::Flow(
            OriginalFlowPolicyReplaceRejection::PolicyKey,
        )));
    };
    let stored = {
        let current = match read_original_current_bytes(view, requested, charge)? {
            None => None,
            Some(bytes) => Some(
                decode_original_flow_policy_current_entry(bytes, &view.safety, limits, charge)
                    .map_err(|error| {
                        ApplyErrorType::Current(OriginalPolicyReadError::Encoding(error))
                    })?
                    .ok_or(ApplyErrorType::Current(OriginalPolicyReadError::Corrupt))?,
            ),
        };
        let stored = current
            .as_ref()
            .map_or(0, super::super::OriginalPolicyEntry::generation);
        pay(charge, 128)?;
        if let Some(error) = guard(expected, stored) {
            return Ok(Err(policy(error)?));
        }
        if let Some(current) = current {
            bind_original_current_type(&current, requested, charge)?;
        }
        stored
    };
    pay(charge, 128)?;
    let context = view.history_max_events_limit.map_err(operational)?;
    if context == 0 || context > MAX_HISTORY_EVENTS {
        return Err(operational(ApplyError::InvalidAuthority));
    }
    let normalized = match prepare_original_flow_policy_layout(
        Cursor::Flow(super::super::RawFlowPolicyValue::Binary(requested)),
        patch,
        context,
        limits,
        charge,
    )
    .map_err(encoding)?
    {
        Ok(layout) => layout,
        Err(reason) => {
            return Ok(Err(FlowReplaceRejected::Flow(
                OriginalFlowPolicyReplaceRejection::Input(reason),
            )));
        }
    };
    let mut high = stored;
    for migrations in [view.active_migrations, view.completed_migrations] {
        pay(charge, plus(128, requested.len())?)?;
        let generation = migrations
            .get(requested)
            .copied()
            .unwrap_or(Ok(0))
            .map_err(operational)?;
        if generation > MAX_POLICY_GENERATION {
            return Err(operational(ApplyError::InvalidAuthority));
        }
        high = high.max(generation);
    }
    pay(charge, 128)?;
    let Some(generation) = high.checked_add(1).filter(|&n| n <= MAX_POLICY_GENERATION) else {
        return Ok(Err(policy(PolicyError::GenerationExhausted)?));
    };
    let encoded =
        emit_original_flow_policy_envelope(&normalized, generation, charge).map_err(encoding)?;
    let policy_key = copy(key, charge)?;
    pay(charge, 128)?;
    Ok(Ok(OriginalFlowPolicyInstall {
        policy_key,
        stored_generation: stored,
        generation,
        encoded,
    }))
}

fn guard(expected: Cursor<'_>, stored: u64) -> Option<PolicyError> {
    if expected.is_nil() {
        return None;
    }
    let Some(value) = expected
        .integer()
        .and_then(|n| u64::try_from(n).ok())
        .filter(|&n| n <= MAX_POLICY_GENERATION)
    else {
        return Some(PolicyError::InvalidGeneration);
    };
    (stored != value).then_some(PolicyError::StaleGeneration)
}

fn policy(error: PolicyError) -> Result<FlowReplaceRejected, ApplyErrorType> {
    PolicyRejection::new(error)
        .map(FlowReplaceRejected::Policy)
        .map_err(|error| ApplyErrorType::Replace(CompletePolicyReplaceApplyError::Outcome(error)))
}

fn pay(charge: &mut PolicySchemaCharge, work: usize) -> Result<(), ApplyErrorType> {
    charge_schema(
        charge,
        u64::try_from(work).map_err(|_| schema(PolicySchemaError::LengthOverflow))?,
        0,
    )
    .map_err(schema)
}
fn plus(a: usize, b: usize) -> Result<usize, ApplyErrorType> {
    a.checked_add(b)
        .ok_or_else(|| schema(PolicySchemaError::LengthOverflow))
}
fn schema(error: PolicySchemaError) -> ApplyErrorType {
    encoding(Error::Schema(error))
}
fn encoding(error: Error) -> ApplyErrorType {
    ApplyErrorType::Replace(CompletePolicyReplaceApplyError::Request(error.into()))
}
fn operational(error: ApplyError) -> ApplyErrorType {
    ApplyErrorType::Replace(CompletePolicyReplaceApplyError::Apply(error))
}
fn copy(bytes: &[u8], charge: &mut PolicySchemaCharge) -> Result<Vec<u8>, ApplyErrorType> {
    pay(charge, plus(128, bytes.len())?)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes.len())
        .map_err(|_| encoding(Error::AllocationFailure))?;
    output.extend_from_slice(bytes);
    Ok(output)
}
