//! Fresh original Replace composition over coherent mathematical inputs.

use std::collections::BTreeMap;
use std::fmt;

use wf_types::version::{LogIdentity, TimestampMillis};

use crate::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use crate::retention::MAX_HISTORY_EVENTS;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;
use crate::workflow::request::gate::GateView;
use crate::workflow::request::{ClientIntent, PolicyRejection, RequestValueError, RetainedOutcome};

use super::admission::charge_schema;
use super::encoding::{
    encode_complete_policy_replace_value, verify_complete_policy_replace_current,
};
use super::patch::check_complete_core_patch_guard;
use super::{
    CompletePolicyData, CompletePolicyReplaceInput, CompletePolicyReplaceRejection,
    CompletePolicyRequestError, CompletePolicyValue, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, check_complete_policy_replace_content,
};

mod retention;

/// Results supplied from one coherent revision; no physical reader is implemented.
#[derive(Clone, Copy)]
pub struct CompletePolicyReplaceApplyView<'view> {
    /// Original namespace, control and retained authority inspected before Fresh.
    pub gate: GateView<'view>,
    /// Exact snapshot revision required by later atomic comparison.
    pub revision: u64,
    /// Present mathematical values or operational failures; missing keys are absent.
    pub policies: &'view BTreeMap<Vec<u8>, Result<CompletePolicyValue, ApplyError>>,
    /// Same-type active migration generations or their original reader failures.
    pub active_migrations: &'view BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    /// Same-type completed migration generations or their original reader failures.
    pub completed_migrations: &'view BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    /// Captured contextual history limit or its original operational failure.
    pub history_max_events_limit: Result<u32, ApplyError>,
    /// Captured original outcome window or its original operational failure.
    pub outcome_retention_ms: Result<u64, ApplyError>,
}

/// Proposed policy/predecessor and immutable original outcome, without persistence.
pub type CompletePolicyReplaceFresh = (Option<(u64, CompletePolicyValue)>, RetainedOutcome);

/// Operational composition refusal; no partial parent or business status is returned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletePolicyReplaceApplyError {
    /// Private request, schema, encoding or continuous-account work refused.
    Request(CompletePolicyRequestError),
    /// Current reader, context, migration or committed control authority refused.
    Apply(ApplyError),
    /// Original outcome construction refused without a retained result.
    Outcome(RequestValueError),
}

impl fmt::Display for CompletePolicyReplaceApplyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(error) => fmt::Display::fmt(error, formatter),
            Self::Apply(error) => fmt::Display::fmt(error, formatter),
            Self::Outcome(error) => fmt::Display::fmt(error, formatter),
        }
    }
}
impl std::error::Error for CompletePolicyReplaceApplyError {}

type ReplacePrepared = Result<(u64, CompletePolicyValue), ReplaceRejected>;

enum ReplaceRejected {
    Policy(PolicyRejection),
    Content(CompletePolicyReplaceRejection),
}

/// Prepares the original replacement and retained outcome on the received account.
///
/// This raw-input mathematical operation verifies no frame and runs no gate.
/// The closed ports parent invokes it only after its actual Fresh decision.
/// Present positive V2 correspondence precedes CAS and requested-type binding;
/// original raw/ETF readers and atomic publication remain outer obligations.
///
/// # Errors
/// Preserves operational failures without any partial plan. Only actual incoming
/// generation decisions and the content checker's descriptor reach retention.
pub fn prepare_complete_policy_replace_fresh(
    view: CompletePolicyReplaceApplyView<'_>,
    input: CompletePolicyReplaceInput,
    committed: (ClientIntent, LogIdentity, TimestampMillis),
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyReplaceFresh, CompletePolicyReplaceApplyError> {
    use CompletePolicyReplaceApplyError::Request;

    if charge.profile() != limits {
        return Err(Request(PolicySchemaError::ProfileMismatch.into()));
    }
    charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
    let (scope, request, _stamp, expected, data) = input.into_parts();
    let prepared = prepare_replace_result(view, data, expected, limits, charge)?;
    retention::retain_replace_outcome(
        scope,
        request,
        committed,
        prepared,
        view.outcome_retention_ms,
        (limits, admission),
        charge,
    )
}

fn prepare_replace_result(
    view: CompletePolicyReplaceApplyView<'_>,
    data: CompletePolicyData,
    expected: Option<u64>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<ReplacePrepared, CompletePolicyReplaceApplyError> {
    use CompletePolicyReplaceApplyError::{Apply, Request};

    let current = read_replace_current(view, &data.workflow_type, limits, charge)?;
    let stored = current.map_or(0, CompletePolicyValue::generation);
    charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
    if let Err(error) = check_complete_core_patch_guard(stored, expected) {
        return Ok(Err(replace_policy_rejection(error)?));
    }
    if let Some(current) = current {
        bind_replace_current_type(current, &data.workflow_type, charge)?;
    }
    charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
    let history_limit = view.history_max_events_limit.map_err(Apply)?;
    if history_limit == 0 || history_limit > MAX_HISTORY_EVENTS {
        return Err(Apply(ApplyError::InvalidAuthority));
    }
    if let Some(rejection) =
        check_complete_policy_replace_content(&data, history_limit, limits, charge)
            .map_err(|error| Request(error.into()))?
    {
        return Ok(Err(ReplaceRejected::Content(rejection)));
    }
    let migration = read_replace_high_water(view, &data.workflow_type, charge)?;
    charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
    let generation = stored.max(migration).checked_add(1);
    let Some(generation) = generation.filter(|value| *value <= MAX_POLICY_GENERATION) else {
        return Ok(Err(replace_policy_rejection(
            PolicyError::GenerationExhausted,
        )?));
    };
    let value = encode_complete_policy_replace_value(generation, data, limits, charge)
        .map_err(|error| Request(error.into()))?;
    Ok(Ok((stored, value)))
}

fn replace_policy_rejection(
    error: PolicyError,
) -> Result<ReplaceRejected, CompletePolicyReplaceApplyError> {
    if !matches!(
        error,
        PolicyError::InvalidGeneration
            | PolicyError::StaleGeneration
            | PolicyError::GenerationExhausted
    ) {
        return Err(CompletePolicyReplaceApplyError::Request(
            PolicySchemaError::InvalidAuthority.into(),
        ));
    }
    PolicyRejection::new(error)
        .map(ReplaceRejected::Policy)
        .map_err(CompletePolicyReplaceApplyError::Outcome)
}

fn read_replace_current<'view>(
    view: CompletePolicyReplaceApplyView<'view>,
    workflow_type: &[u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<&'view CompletePolicyValue>, CompletePolicyReplaceApplyError> {
    use CompletePolicyReplaceApplyError::{Apply, Request};

    let work = u64::try_from(workflow_type.len())
        .ok()
        .and_then(|length| length.checked_add(128))
        .ok_or(Request(PolicySchemaError::LengthOverflow.into()))?;
    charge_schema(charge, work, 0).map_err(|error| Request(error.into()))?;
    match view.policies.get(workflow_type) {
        Some(Ok(value)) => {
            verify_complete_policy_replace_current(value, limits, charge)
                .map_err(|error| Request(error.into()))?;
            Ok(Some(value))
        }
        Some(Err(error)) => Err(Apply(*error)),
        None => Ok(None),
    }
}

fn bind_replace_current_type(
    current: &CompletePolicyValue,
    requested: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<(), CompletePolicyReplaceApplyError> {
    use CompletePolicyReplaceApplyError::Request;

    let work = requested
        .len()
        .checked_add(current.data().workflow_type.len())
        .and_then(|length| u64::try_from(length).ok())
        .and_then(|length| length.checked_add(128))
        .ok_or(Request(PolicySchemaError::LengthOverflow.into()))?;
    charge_schema(charge, work, 0).map_err(|error| Request(error.into()))?;
    if current.data().workflow_type != requested {
        return Err(Request(PolicySchemaError::InvalidAuthority.into()));
    }
    Ok(())
}

fn read_replace_high_water(
    view: CompletePolicyReplaceApplyView<'_>,
    workflow_type: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<u64, CompletePolicyReplaceApplyError> {
    use CompletePolicyReplaceApplyError::{Apply, Request};

    let work = u64::try_from(workflow_type.len())
        .ok()
        .and_then(|length| length.checked_add(128))
        .ok_or(Request(PolicySchemaError::LengthOverflow.into()))?;
    let mut high_water = 0;
    for migrations in [view.active_migrations, view.completed_migrations] {
        charge_schema(charge, work, 0).map_err(|error| Request(error.into()))?;
        let generation = match migrations.get(workflow_type) {
            Some(value) => (*value).map_err(Apply)?,
            None => 0,
        };
        if generation > MAX_POLICY_GENERATION {
            return Err(Apply(ApplyError::InvalidAuthority));
        }
        high_water = high_water.max(generation);
    }
    Ok(high_water)
}
