//! Fresh core-Patch composition over supplied coherent mathematical authority.

use std::collections::BTreeMap;
use std::fmt;

use wf_types::identity::RequestId;
use wf_types::version::{LogIdentity, TimestampMillis};

use crate::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use crate::retention::{MAX_HISTORY_EVENTS, RetentionError};
use crate::value::{DomainError, InputField, RetryPolicyField};
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;
use crate::workflow::request::gate::GateView;
use crate::workflow::request::{
    ClientIntent, PolicyRejection, RequestScope, RequestValueError, RetainedMetadata,
    RetainedOutcome, RetainedStatus,
};

use super::admission::charge_schema;
use super::patch::{
    check_complete_core_patch_guard, encode_core_patch_content, merge_core_patch_content,
    restore_core_patch_current, validate_complete_core_patch_current,
};
use super::{
    CompleteCorePolicyPatchRequestInput, CompletePolicyEncodingError, CompletePolicyRequestError,
    CompletePolicyValue, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};

/// Supplied results from one coherent namespace/revision; no reader is implemented.
#[derive(Clone, Copy)]
pub struct CompleteCorePolicyPatchApplyView<'view> {
    /// Retained/control authority inspected before current policy materialization.
    pub gate: GateView<'view>,
    /// Required current snapshot revision, with no missing-value sentinel.
    pub revision: u64,
    /// Present values or operational read refusals; absent keys mean actual absence.
    pub policies: &'view BTreeMap<Vec<u8>, Result<CompletePolicyValue, ApplyError>>,
    /// Same-type active migration generations or operational reader refusals.
    pub active_migrations: &'view BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    /// Same-type completed migration generations or operational reader refusals.
    pub completed_migrations: &'view BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    /// Captured contextual history bound or its original operational refusal.
    pub history_max_events_limit: Result<u32, ApplyError>,
    /// Captured original outcome window or its original operational refusal.
    pub outcome_retention_ms: Result<u64, ApplyError>,
}

/// Proposed optional policy/predecessor and immutable original retained outcome.
pub type CompleteCorePolicyPatchFresh = (Option<(u64, CompletePolicyValue)>, RetainedOutcome);

/// Operational composition failure; no partial parent is returned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompleteCorePolicyPatchApplyError {
    /// Private request, complete schema, encoding or continuous-account refusal.
    Request(CompletePolicyRequestError),
    /// Current/read/context/control authority failed operationally.
    Apply(ApplyError),
    /// Original outcome construction failed without a retained status.
    Outcome(RequestValueError),
}

impl fmt::Display for CompleteCorePolicyPatchApplyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(error) => fmt::Display::fmt(error, formatter),
            Self::Apply(error) => fmt::Display::fmt(error, formatter),
            Self::Outcome(error) => fmt::Display::fmt(error, formatter),
        }
    }
}
impl std::error::Error for CompleteCorePolicyPatchApplyError {}

/// Freshly prepares mathematical policy/outcome owners on the received account.
///
/// This raw-input composition performs no request gate, verification or storage.
/// The closed ports parent invokes it only after verification and its one Fresh
/// decision. Present current validation/restoration failures return operationally.
///
/// # Errors
/// Preserves operational read/work/encoding/outcome failures without a partial plan.
/// Only the fixed new-command business registry produces a rejected outcome.
pub fn prepare_complete_core_policy_patch_fresh(
    view: CompleteCorePolicyPatchApplyView<'_>,
    input: CompleteCorePolicyPatchRequestInput,
    committed: (ClientIntent, LogIdentity, TimestampMillis),
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompleteCorePolicyPatchFresh, CompleteCorePolicyPatchApplyError> {
    use CompleteCorePolicyPatchApplyError::{Apply, Request};

    if charge.profile() != limits {
        return Err(Request(PolicySchemaError::ProfileMismatch.into()));
    }
    charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
    let (scope, request, _stamp, command) = input.into_parts();
    let lookup_work = u64::try_from(command.workflow_type.len())
        .ok()
        .and_then(|length| length.checked_add(128))
        .ok_or(Request(PolicySchemaError::LengthOverflow.into()))?;
    charge_schema(charge, lookup_work, 0).map_err(|error| Request(error.into()))?;
    let existing = match view.policies.get(command.workflow_type.as_slice()) {
        Some(Ok(value)) => Some(value),
        Some(Err(error)) => return Err(Apply(*error)),
        None => None,
    };
    if let Some(existing) = existing {
        validate_complete_core_patch_current(existing, &command.workflow_type, limits, charge)
            .map_err(|error| Request(error.into()))?;
    }
    charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
    let stored = existing.map_or(0, CompletePolicyValue::generation);
    let prepared = 'prepare: {
        if let Err(error) = check_complete_core_patch_guard(stored, command.expected_generation) {
            let error = match error {
                PolicyError::InvalidAuthority => PolicySchemaError::InvalidAuthority,
                error => PolicySchemaError::Policy(error),
            };
            break 'prepare Err(Request(error.into()));
        }
        let data = if let Some(existing) = existing {
            restore_core_patch_current(Some(existing), command.workflow_type, limits, charge)
                .map_err(|error| Request(error.into()))?
        } else {
            match restore_core_patch_current(None, command.workflow_type, limits, charge) {
                Ok(data) => data,
                Err(error) => break 'prepare Err(Request(error.into())),
            }
        };
        charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
        let history_limit = view.history_max_events_limit.map_err(Apply)?;
        if history_limit == 0 || history_limit > MAX_HISTORY_EVENTS {
            return Err(Apply(ApplyError::InvalidAuthority));
        }
        let data = match merge_core_patch_content(
            data,
            command.patch,
            existing.is_some(),
            history_limit,
            limits,
            charge,
        ) {
            Ok(data) => data,
            Err(error) => break 'prepare Err(Request(error.into())),
        };
        charge_schema(charge, lookup_work, 0).map_err(|error| Request(error.into()))?;
        let active = match view.active_migrations.get(data.workflow_type.as_slice()) {
            Some(value) => (*value).map_err(Apply)?,
            None => 0,
        };
        if active > MAX_POLICY_GENERATION {
            return Err(Apply(ApplyError::InvalidAuthority));
        }
        charge_schema(charge, lookup_work, 0).map_err(|error| Request(error.into()))?;
        let completed = match view.completed_migrations.get(data.workflow_type.as_slice()) {
            Some(value) => (*value).map_err(Apply)?,
            None => 0,
        };
        if completed > MAX_POLICY_GENERATION {
            return Err(Apply(ApplyError::InvalidAuthority));
        }
        encode_core_patch_content(stored, active.max(completed), data, limits, charge)
            .map(|value| (stored, value))
            .map_err(|error| Request(error.into()))
    };
    retain_core_patch_outcome(
        scope,
        request,
        committed,
        prepared,
        view.outcome_retention_ms,
        admission,
        charge,
    )
}

fn core_patch_business_message(error: PolicyError) -> Option<&'static [u8]> {
    Some(match error {
        PolicyError::InvalidVersion => {
            b"ERR flow policy version must be a non-empty string or non-negative integer"
        }
        PolicyError::InvalidMaxActive => {
            b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity"
        }
        PolicyError::InvalidStateMode => b"ERR flow state mode must be parallel or fifo",
        PolicyError::InvalidStatePolicy => b"ERR flow state policy must be a map or keyword list",
        PolicyError::InvalidGeneration => b"ERR invalid flow policy generation",
        PolicyError::StaleGeneration => b"ERR stale flow policy generation",
        PolicyError::GenerationExhausted => b"ERR flow policy generation exhausted",
        PolicyError::Retention(RetentionError::Ttl) => {
            b"ERR flow retention ttl_ms must be between 1 and 31536000000"
        }
        PolicyError::Retention(RetentionError::History) => {
            b"ERR flow retention history_max_events is outside configured bounds"
        }
        PolicyError::Domain(DomainError::EmptyValue(InputField::Type)) => {
            b"ERR flow type must be a non-empty string"
        }
        PolicyError::Domain(DomainError::InvalidRetryPolicy(RetryPolicyField::MaxRetries)) => {
            b"ERR flow retry max_retries must be between 0 and 1000"
        }
        PolicyError::Domain(DomainError::InvalidRetryPolicy(RetryPolicyField::BaseMillis)) => {
            b"ERR flow retry base_ms must be between 0 and 2592000000"
        }
        PolicyError::Domain(DomainError::InvalidRetryPolicy(RetryPolicyField::MaxMillis)) => {
            b"ERR flow retry max_ms must be between 0 and 2592000000"
        }
        PolicyError::Domain(DomainError::InvalidRetryPolicy(RetryPolicyField::JitterPercent)) => {
            b"ERR flow retry jitter_pct must be between 0 and 100"
        }
        PolicyError::Domain(DomainError::InvalidRetryPolicy(RetryPolicyField::ExhaustedTo)) => {
            b"ERR flow retry exhausted_to must be a non-empty string"
        }
        PolicyError::Domain(DomainError::InvalidRetryPolicy(
            RetryPolicyField::RunningExhaustion,
        )) => b"ERR flow retry exhausted_to cannot be running",
        PolicyError::Domain(DomainError::InvalidRetryPolicy(RetryPolicyField::State)) => {
            b"ERR flow policy state must be a non-empty string"
        }
        PolicyError::Domain(DomainError::InvalidRetryPolicy(RetryPolicyField::RunningState)) => {
            b"ERR flow policy state cannot be running"
        }
        _ => return None,
    })
}

fn core_patch_parent_bytes(
    scope: &RequestScope,
    request: &RequestId,
    installed: Option<&CompletePolicyValue>,
    response_len: usize,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), CompleteCorePolicyPatchApplyError> {
    use CompleteCorePolicyPatchApplyError::Request;

    charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
    let bytes = [
        scope.tenant().as_bytes().len(),
        scope.group().as_bytes().len(),
        scope.recovery().as_bytes().len(),
        request.as_bytes().len(),
        installed.map_or(0, |value| value.encoded().len()),
        response_len,
    ]
    .into_iter()
    .try_fold(0_usize, usize::checked_add)
    .ok_or(Request(PolicySchemaError::LengthOverflow.into()))?;
    if bytes > admission.decision_bytes() {
        return Err(Request(CompletePolicyEncodingError::ByteLimit.into()));
    }
    Ok(())
}

fn core_patch_expiry(
    window: Result<u64, ApplyError>,
    now: TimestampMillis,
    charge: &mut PolicySchemaCharge,
) -> Result<TimestampMillis, CompleteCorePolicyPatchApplyError> {
    use CompleteCorePolicyPatchApplyError::{Apply, Outcome, Request};

    charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
    let window = window.map_err(Apply)?;
    if window == 0 {
        return Err(Outcome(RequestValueError::InvalidExpiry));
    }
    now.checked_add(window)
        .ok_or(Outcome(RequestValueError::InvalidExpiry))
}

fn retain_core_patch_outcome(
    scope: RequestScope,
    request: RequestId,
    committed: (ClientIntent, LogIdentity, TimestampMillis),
    prepared: Result<(u64, CompletePolicyValue), CompleteCorePolicyPatchApplyError>,
    window: Result<u64, ApplyError>,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompleteCorePolicyPatchFresh, CompleteCorePolicyPatchApplyError> {
    use CompleteCorePolicyPatchApplyError::{Outcome, Request};

    let (status, response) = match &prepared {
        Ok((_, value)) => (RetainedStatus::Accepted, value.encoded()),
        Err(
            original @ Request(CompletePolicyRequestError::Encoding(
                CompletePolicyEncodingError::Schema(PolicySchemaError::Policy(error)),
            )),
        ) => {
            let Some(message) = core_patch_business_message(*error) else {
                return Err(*original);
            };
            let rejection = PolicyRejection::new(*error).map_err(Outcome)?;
            (RetainedStatus::PolicyRejected(rejection), message)
        }
        Err(error) => return Err(*error),
    };
    let expires_at = core_patch_expiry(window, committed.2, charge)?;
    core_patch_parent_bytes(
        &scope,
        &request,
        prepared.as_ref().ok().map(|(_, value)| value),
        response.len(),
        admission,
        charge,
    )?;
    charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
    let metadata = RetainedMetadata {
        scope,
        request,
        intent: committed.0,
        original_position: committed.1,
        original_time: committed.2,
        expires_at,
        status,
    };
    let work = u64::try_from(response.len())
        .ok()
        .and_then(|length| length.checked_add(128))
        .ok_or(Request(PolicySchemaError::LengthOverflow.into()))?;
    charge_schema(charge, work, 0).map_err(|error| Request(error.into()))?;
    let mut copied = Vec::new();
    copied
        .try_reserve_exact(response.len())
        .map_err(|_| Request(CompletePolicyEncodingError::AllocationFailure.into()))?;
    copied.extend_from_slice(response);
    charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
    let outcome = RetainedOutcome::new(metadata, 1, copied, admission).map_err(Outcome)?;
    Ok((prepared.ok(), outcome))
}
