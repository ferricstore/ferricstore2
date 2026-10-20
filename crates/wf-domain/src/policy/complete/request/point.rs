//! Current-only mathematical Fresh authority, sharing the original ordered reducer.

use super::super::patch::CompleteCorePolicyPatchInput;
use super::{
    AdmissionLimits, ApplyError, ClientIntent, CompleteCorePolicyPatchApplyError,
    CompleteCorePolicyPatchApplyView, CompleteCorePolicyPatchFresh,
    CompleteCorePolicyPatchPreparation, CompleteCorePolicyPatchRequestInput, LogIdentity,
    MAX_HISTORY_EVENTS, MAX_POLICY_GENERATION, PolicyError, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, StoredPolicyValue, TimestampMillis, charge_schema,
    check_complete_core_patch_guard, encode_core_patch_content, merge_core_patch_content,
    restore_core_patch_current, retain_core_patch_outcome, validate_complete_core_patch_current,
};

/// Supplied same-revision current/context results; no storage authority is minted.
#[derive(Clone, Copy)]
pub struct CompleteCorePolicyPatchFreshView<'a> {
    /// Exact requested workflow type, independently borrowed through decision.
    pub workflow_type: &'a [u8],
    /// Complete current value or its original operational refusal.
    pub current: Result<Option<&'a StoredPolicyValue>, ApplyError>,
    /// Actual closed-schema absence or known active migration generation.
    pub active_migration: Result<Option<u64>, ApplyError>,
    /// Actual closed-schema absence or known completed migration generation.
    pub completed_migration: Result<Option<u64>, ApplyError>,
    /// Captured immutable server history bound.
    pub history_max_events_limit: Result<u32, ApplyError>,
    /// Captured immutable original outcome window.
    pub outcome_retention_ms: Result<u64, ApplyError>,
}

struct CorePatchPreparationContext {
    query: Option<Vec<u8>>,
    history: Option<u32>,
    migrations: Option<(u64, u64)>,
}

pub(super) trait CorePatchAuthority<'a> {
    fn current(&self, kind: &[u8]) -> Result<Option<&'a StoredPolicyValue>, ApplyError>;
    fn history_limit(&self) -> Result<u32, ApplyError>;
    fn active(&self, kind: &[u8]) -> Result<Option<u64>, ApplyError>;
    fn completed(&self, kind: &[u8]) -> Result<Option<u64>, ApplyError>;
    fn outcome_window(&self) -> Result<u64, ApplyError>;
}
impl<'a> CorePatchAuthority<'a> for CompleteCorePolicyPatchApplyView<'a> {
    fn current(&self, kind: &[u8]) -> Result<Option<&'a StoredPolicyValue>, ApplyError> {
        self.policies
            .get(kind)
            .map(|v| v.as_ref().map_err(|e| *e))
            .transpose()
    }
    fn history_limit(&self) -> Result<u32, ApplyError> {
        self.history_max_events_limit
    }
    fn active(&self, kind: &[u8]) -> Result<Option<u64>, ApplyError> {
        self.active_migrations.get(kind).copied().transpose()
    }
    fn completed(&self, kind: &[u8]) -> Result<Option<u64>, ApplyError> {
        self.completed_migrations.get(kind).copied().transpose()
    }
    fn outcome_window(&self) -> Result<u64, ApplyError> {
        self.outcome_retention_ms
    }
}
impl<'a> CorePatchAuthority<'a> for CompleteCorePolicyPatchFreshView<'a> {
    fn current(&self, kind: &[u8]) -> Result<Option<&'a StoredPolicyValue>, ApplyError> {
        if self.workflow_type != kind {
            return Err(ApplyError::InvalidAuthority);
        }
        self.current
    }
    fn history_limit(&self) -> Result<u32, ApplyError> {
        self.history_max_events_limit
    }
    fn active(&self, _kind: &[u8]) -> Result<Option<u64>, ApplyError> {
        self.active_migration
    }
    fn completed(&self, _kind: &[u8]) -> Result<Option<u64>, ApplyError> {
        self.completed_migration
    }
    fn outcome_window(&self) -> Result<u64, ApplyError> {
        self.outcome_retention_ms
    }
}

/// Prepares Fresh current-only mathematical owners on the received account.
/// # Errors
/// Preserves current, context, schema, retention and new-content diagnoses.
pub fn prepare_complete_core_policy_patch_point_fresh(
    view: CompleteCorePolicyPatchFreshView<'_>,
    input: CompleteCorePolicyPatchRequestInput,
    committed: (ClientIntent, LogIdentity, TimestampMillis),
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompleteCorePolicyPatchFresh, CompleteCorePolicyPatchApplyError> {
    prepare(&view, input, committed, limits, admission, charge)
}

/// Prepares Fresh owners while retaining the original query and current backing.
///
/// The returned current is the same validated view borrow, not stored authority.
/// Final slots record only successfully reached, range-valid context getters.
/// # Errors
/// Preserves the original preparation order and continuous-account diagnoses.
pub fn prepare_complete_core_policy_patch_point_fresh_with_query<'current>(
    view: CompleteCorePolicyPatchFreshView<'current>,
    input: CompleteCorePolicyPatchRequestInput,
    committed: (ClientIntent, LogIdentity, TimestampMillis),
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompleteCorePolicyPatchPreparation<'current>, CompleteCorePolicyPatchApplyError> {
    prepare_with_query(&view, input, committed, limits, admission, charge)
}

pub(super) fn prepare<'a>(
    view: &impl CorePatchAuthority<'a>,
    input: CompleteCorePolicyPatchRequestInput,
    committed: (ClientIntent, LogIdentity, TimestampMillis),
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompleteCorePolicyPatchFresh, CompleteCorePolicyPatchApplyError> {
    prepare_with_query(view, input, committed, limits, admission, charge)
        .map(|(fresh, _, _, _, _)| fresh)
}

pub(super) fn prepare_with_query<'a>(
    view: &impl CorePatchAuthority<'a>,
    input: CompleteCorePolicyPatchRequestInput,
    committed: (ClientIntent, LogIdentity, TimestampMillis),
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompleteCorePolicyPatchPreparation<'a>, CompleteCorePolicyPatchApplyError> {
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
    let existing = view.current(&command.workflow_type).map_err(Apply)?;
    let raw = if let Some(StoredPolicyValue::Original(bytes)) = existing {
        Some(
            crate::policy::complete::flow_input::read_raw(
                bytes,
                &command.workflow_type,
                limits,
                charge,
                0,
            )
            .map_err(|error| Request(error.into()))?,
        )
    } else {
        None
    };
    if let Some(StoredPolicyValue::Complete(existing)) = existing {
        validate_complete_core_patch_current(existing, &command.workflow_type, limits, charge)
            .map_err(|error| Request(error.into()))?;
    }
    charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
    let mut context = CorePatchPreparationContext {
        query: None,
        history: None,
        migrations: None,
    };
    let prepared = prepare_content(
        view,
        command,
        (existing, raw, lookup_work),
        limits,
        charge,
        &mut context,
    );
    let fresh = retain_core_patch_outcome(
        scope,
        request,
        committed,
        prepared,
        view.outcome_window(),
        admission,
        charge,
    )?;
    Ok((
        fresh,
        context.query,
        existing,
        context.history,
        context.migrations,
    ))
}

fn prepare_content<'a>(
    view: &impl CorePatchAuthority<'a>,
    command: CompleteCorePolicyPatchInput,
    current: (
        Option<&StoredPolicyValue>,
        Option<
            crate::policy::complete::original_entry::indexed::OriginalPolicyCurrentIndex<
                '_,
                'static,
            >,
        >,
        u64,
    ),
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    context: &mut CorePatchPreparationContext,
) -> Result<(u64, StoredPolicyValue), CompleteCorePolicyPatchApplyError> {
    use CompleteCorePolicyPatchApplyError::{Apply, Request};
    let (existing, raw, lookup_work) = current;
    if let Some(raw) = raw {
        return prepare_raw_content(view, command, raw, lookup_work, limits, charge, context);
    }
    let existing = existing.and_then(|value| match value {
        StoredPolicyValue::Complete(value) => Some(value.as_ref()),
        StoredPolicyValue::Original(_) => None,
    });
    let stored = existing.map_or(0, super::super::CompletePolicyValue::generation);
    if let Err(error) = check_complete_core_patch_guard(stored, command.expected_generation) {
        let error = match error {
            PolicyError::InvalidAuthority => PolicySchemaError::InvalidAuthority,
            error => PolicySchemaError::Policy(error),
        };
        context.query = Some(command.workflow_type);
        return Err(Request(error.into()));
    }
    let data = match restore_core_patch_current(existing, command.workflow_type, limits, charge) {
        Ok(data) => data,
        Err((error, query)) => {
            context.query = Some(query);
            return Err(Request(error.into()));
        }
    };
    charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
    let history_limit = view.history_limit().map_err(Apply)?;
    if history_limit == 0 || history_limit > MAX_HISTORY_EVENTS {
        return Err(Apply(ApplyError::InvalidAuthority));
    }
    context.history = Some(history_limit);
    let data = match merge_core_patch_content(
        data,
        command.patch,
        existing.is_some(),
        history_limit,
        limits,
        charge,
    ) {
        Ok(data) => data,
        Err((error, query)) => {
            context.query = Some(query);
            return Err(Request(error.into()));
        }
    };
    charge_schema(charge, lookup_work, 0).map_err(|error| Request(error.into()))?;
    let active = view
        .active(&data.workflow_type)
        .map_err(Apply)?
        .unwrap_or(0);
    if active > MAX_POLICY_GENERATION {
        return Err(Apply(ApplyError::InvalidAuthority));
    }
    charge_schema(charge, lookup_work, 0).map_err(|error| Request(error.into()))?;
    let completed = view
        .completed(&data.workflow_type)
        .map_err(Apply)?
        .unwrap_or(0);
    if completed > MAX_POLICY_GENERATION {
        return Err(Apply(ApplyError::InvalidAuthority));
    }
    context.migrations = Some((active, completed));
    encode_core_patch_content(stored, active.max(completed), data, limits, charge)
        .map(|value| (stored, StoredPolicyValue::Complete(Box::new(value))))
        .map_err(|(error, query)| {
            context.query = Some(query);
            Request(error.into())
        })
}

fn prepare_raw_content<'a>(
    view: &impl CorePatchAuthority<'a>,
    command: CompleteCorePolicyPatchInput,
    mut index: crate::policy::complete::original_entry::indexed::OriginalPolicyCurrentIndex<
        '_,
        'static,
    >,
    lookup_work: u64,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    context: &mut CorePatchPreparationContext,
) -> Result<(u64, StoredPolicyValue), CompleteCorePolicyPatchApplyError> {
    use crate::policy::complete::flow_input::{admit_raw, encode_raw, measure_raw};
    use CompleteCorePolicyPatchApplyError::{Apply, Request};
    let stored = index.entry().generation();
    let prepared = (|| {
        check_complete_core_patch_guard(stored, command.expected_generation)
            .map_err(|error| Request(PolicySchemaError::Policy(error).into()))?;
        charge_schema(charge, 128, 0).map_err(|e| Request(e.into()))?;
        let history = view.history_limit().map_err(Apply)?;
        if history == 0 || history > MAX_HISTORY_EVENTS {
            return Err(Apply(ApplyError::InvalidAuthority));
        }
        context.history = Some(history);
        admit_raw(&command, limits, charge).map_err(|e| Request(e.into()))?;
        let measured = measure_raw(&mut index, &command, Some(history), limits, charge)
            .map_err(|e| Request(e.into()))?;
        charge_schema(charge, lookup_work, 0).map_err(|e| Request(e.into()))?;
        let active = view
            .active(&command.workflow_type)
            .map_err(Apply)?
            .unwrap_or(0);
        if active > MAX_POLICY_GENERATION {
            return Err(Apply(ApplyError::InvalidAuthority));
        }
        charge_schema(charge, lookup_work, 0).map_err(|e| Request(e.into()))?;
        let completed = view
            .completed(&command.workflow_type)
            .map_err(Apply)?
            .unwrap_or(0);
        if completed > MAX_POLICY_GENERATION {
            return Err(Apply(ApplyError::InvalidAuthority));
        }
        context.migrations = Some((active, completed));
        let generation = stored
            .max(active)
            .max(completed)
            .checked_add(1)
            .filter(|&g| g <= MAX_POLICY_GENERATION)
            .ok_or(Request(
                PolicySchemaError::Policy(PolicyError::GenerationExhausted).into(),
            ))?;
        encode_raw(
            &mut index, &command, generation, measured, limits, charge, 0,
        )
        .map_err(|e| Request(e.into()))
    })();
    context.query = Some(command.workflow_type);
    prepared.map(|bytes| (stored, StoredPolicyValue::Original(bytes)))
}
