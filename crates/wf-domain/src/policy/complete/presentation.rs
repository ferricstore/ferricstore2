//! Borrowed public policy response values with captured configuration defaults.

use crate::policy::definition::{PolicyVersion, StateMode};
use crate::policy::{Backoff, RetryOverride};
use crate::retention::{MAX_HISTORY_EVENTS, ResolvedRetention, RetentionOverride};

use super::admission::charge_schema;
use super::{
    CompletePolicyData, CompletePolicyEncodingError, CompletePolicyValue, CompleteStatePolicyData,
    GovernanceData, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    verify_complete_policy_value_v2,
};

/// Resolved retry scalars and an original or static exhaustion target.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct PresentedRetry<'v> {
    /// Inclusive retry count after default, type and state resolution.
    pub max_retries: u16,
    /// Fully normalized selected backoff.
    pub backoff: Backoff,
    /// Original selected owner, or the static source default.
    pub exhausted_to: &'v [u8],
}

/// One declared state in an all-state policy response.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct PresentedPolicyState<'v> {
    /// Original opaque declared state name.
    pub state: &'v [u8],
    /// State execution mode, defaulting to parallel.
    pub mode: StateMode,
    /// Type-only duration; absence means infinity.
    pub max_active_ms: Option<u64>,
    /// Default, type and state retry resolution.
    pub retry: PresentedRetry<'v>,
    /// Public retention, with no internal hot-history field.
    pub retention: ResolvedRetention,
    /// Only this state's raw group; type governance is not inherited here.
    pub governance: Option<&'v GovernanceData>,
}

/// Pure presentation; no transport, installed authority or retained replay proof.
///
/// Query bytes and immutable policy leaves have independent owner lifetimes.
/// The all-state vector owns descriptors only, never copied policy leaves.
#[derive(Eq, PartialEq)]
pub enum CompletePolicyPresentation<'v, 'q> {
    /// Type response including every declared state in binary key order.
    All {
        /// Original admitted requested workflow type.
        workflow_type: &'q [u8],
        /// Stored internal generation, or zero for actual absence.
        generation: u64,
        /// Original customer version without numeric filtering.
        version: Option<&'v PolicyVersion>,
        /// Type-only duration; absence means infinity.
        max_active_ms: Option<u64>,
        /// Default and type retry resolution.
        retry: PresentedRetry<'v>,
        /// Captured default and type public retention.
        retention: ResolvedRetention,
        /// Original normalized names; absence presents as an empty list.
        indexed_attributes: &'v [Vec<u8>],
        /// Original optional normalized metadata key.
        indexed_state_meta: Option<&'v [u8]>,
        /// Original optional type governance group.
        governance: Option<&'v GovernanceData>,
        /// Every declared state; each row retains its original leaf owners.
        states: Vec<PresentedPolicyState<'v>>,
    },
    /// Selected-state response; unknown and empty states use type fallback.
    State {
        /// Original admitted requested workflow type.
        workflow_type: &'q [u8],
        /// Original query bytes, independent of the policy owner's lifetime.
        state: &'q [u8],
        /// Stored internal generation, or zero for actual absence.
        generation: u64,
        /// Original customer version without numeric filtering.
        version: Option<&'v PolicyVersion>,
        /// Selected state mode, defaulting to parallel.
        mode: StateMode,
        /// Type-only duration; absence means infinity.
        max_active_ms: Option<u64>,
        /// Default, type and selected-state retry resolution.
        retry: PresentedRetry<'v>,
        /// Captured default, type and selected-state public retention.
        retention: ResolvedRetention,
        /// Original normalized names; absence presents as an empty list.
        indexed_attributes: &'v [Vec<u8>],
        /// Original optional normalized metadata key.
        indexed_state_meta: Option<&'v [u8]>,
        /// Whole state group if present, otherwise the whole type group.
        governance: Option<&'v GovernanceData>,
    },
}

/// Presents a complete normalized policy or explicitly absent policy.
///
/// Captured public retention defaults are admitted inputs; this function reads
/// no environment. It verifies present V2 content once on the received account.
/// Raw governance groups are presented, without per-field governance resolution.
///
/// # Errors
/// Preserves profile, current encoding and prospective work failures. Invalid
/// type/default/query operands or a foreign current type return invalid authority.
/// No partial response is returned; admitted work and prior peak remain paid.
pub fn present_complete_policy<'v, 'q>(
    workflow_type: &'q [u8],
    value: Option<&'v CompletePolicyValue>,
    state: Option<&'q [u8]>,
    defaults: ResolvedRetention,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyPresentation<'v, 'q>, CompletePolicyEncodingError> {
    use CompletePolicyEncodingError::Schema;

    if charge.profile() != limits {
        return Err(Schema(PolicySchemaError::ProfileMismatch));
    }
    charge_schema(charge, 128, 0).map_err(Schema)?;
    let default_overrides = RetentionOverride {
        ttl_ms: Some(defaults.ttl_ms),
        history_max_events: Some(defaults.history_max_events),
    };
    if workflow_type.is_empty()
        || workflow_type.len() > 262_144
        || default_overrides.validate(MAX_HISTORY_EVENTS).is_err()
    {
        return Err(Schema(PolicySchemaError::InvalidAuthority));
    }
    let n = u64::try_from(workflow_type.len())
        .map_err(|_| Schema(PolicySchemaError::LengthOverflow))?;
    charge_schema(charge, n, 0).map_err(Schema)?;
    if let Some(value) = value {
        verify_complete_policy_value_v2(value, limits, charge)?;
        charge_schema(
            charge,
            n.checked_add(128)
                .ok_or(Schema(PolicySchemaError::LengthOverflow))?,
            0,
        )
        .map_err(Schema)?;
        if value.data().workflow_type != workflow_type {
            return Err(Schema(PolicySchemaError::InvalidAuthority));
        }
    }
    present_projection(workflow_type, value, state, defaults, charge)
}

fn present_projection<'v, 'q>(
    workflow_type: &'q [u8],
    value: Option<&'v CompletePolicyValue>,
    state: Option<&'q [u8]>,
    defaults: ResolvedRetention,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyPresentation<'v, 'q>, CompletePolicyEncodingError> {
    use CompletePolicyEncodingError::Schema;

    let data = value.map(CompletePolicyValue::data);
    let selected = if let Some(state) = state {
        if state.len() > 262_144 {
            return Err(Schema(PolicySchemaError::InvalidAuthority));
        }
        let n =
            u64::try_from(state.len()).map_err(|_| Schema(PolicySchemaError::LengthOverflow))?;
        charge_schema(
            charge,
            n.checked_add(128)
                .ok_or(Schema(PolicySchemaError::LengthOverflow))?,
            0,
        )
        .map_err(Schema)?;
        data.and_then(|data| data.states.get(state))
    } else {
        None
    };
    charge_schema(charge, 128, 0).map_err(Schema)?;
    let generation = value.map_or(0, CompletePolicyValue::generation);
    let version = data.and_then(|data| data.version.as_ref());
    let max_active_ms = data.and_then(|data| data.max_active_ms);
    let retry = present_retry(data.and_then(|data| data.retry.as_ref()), selected);
    let retention = present_retention(defaults, data.and_then(|data| data.retention), selected);
    let indexed_attributes = data
        .and_then(|data| data.indexed_attributes.as_deref())
        .unwrap_or(&[]);
    let indexed_state_meta = data.and_then(|data| data.indexed_state_meta.as_deref());
    let governance = data.and_then(|data| data.governance.as_ref());
    if let Some(state) = state {
        return Ok(CompletePolicyPresentation::State {
            workflow_type,
            state,
            generation,
            version,
            mode: selected
                .and_then(|state| state.mode)
                .unwrap_or(StateMode::Parallel),
            max_active_ms,
            retry,
            retention,
            indexed_attributes,
            indexed_state_meta,
            governance: selected
                .and_then(|state| state.governance.as_ref())
                .or(governance),
        });
    }
    let count = data.map_or(0, |data| data.states.len());
    let reserve = u64::try_from(count)
        .ok()
        .and_then(|count| count.checked_mul(128))
        .and_then(|work| work.checked_add(128))
        .ok_or(Schema(PolicySchemaError::LengthOverflow))?;
    charge_schema(charge, reserve, 0).map_err(Schema)?;
    let mut states = Vec::new();
    states
        .try_reserve_exact(count)
        .map_err(|_| CompletePolicyEncodingError::AllocationFailure)?;
    if let Some(data) = data {
        for (name, state) in &data.states {
            let n =
                u64::try_from(name.len()).map_err(|_| Schema(PolicySchemaError::LengthOverflow))?;
            charge_schema(
                charge,
                n.checked_add(128)
                    .ok_or(Schema(PolicySchemaError::LengthOverflow))?,
                0,
            )
            .map_err(Schema)?;
            states.push(present_state(name, data, state, defaults));
        }
    }
    Ok(CompletePolicyPresentation::All {
        workflow_type,
        generation,
        version,
        max_active_ms,
        retry,
        retention,
        indexed_attributes,
        indexed_state_meta,
        governance,
        states,
    })
}

fn present_retry<'v>(
    type_retry: Option<&'v RetryOverride>,
    state: Option<&'v CompleteStatePolicyData>,
) -> PresentedRetry<'v> {
    let state_retry = state.and_then(|state| state.retry.as_ref());
    PresentedRetry {
        max_retries: state_retry
            .and_then(|retry| retry.max_retries)
            .or_else(|| type_retry.and_then(|retry| retry.max_retries))
            .unwrap_or(3),
        backoff: state_retry
            .and_then(|retry| retry.backoff)
            .or_else(|| type_retry.and_then(|retry| retry.backoff))
            .unwrap_or_default(),
        exhausted_to: state_retry
            .and_then(|retry| retry.exhausted_to.as_deref())
            .or_else(|| type_retry.and_then(|retry| retry.exhausted_to.as_deref()))
            .unwrap_or(b"failed"),
    }
}

fn present_retention(
    mut defaults: ResolvedRetention,
    type_retention: Option<RetentionOverride>,
    state: Option<&CompleteStatePolicyData>,
) -> ResolvedRetention {
    for retention in [type_retention, state.and_then(|state| state.retention)]
        .into_iter()
        .flatten()
    {
        crate::retention::merge(&mut defaults, retention);
    }
    defaults
}

fn present_state<'v>(
    name: &'v [u8],
    data: &'v CompletePolicyData,
    state: &'v CompleteStatePolicyData,
    defaults: ResolvedRetention,
) -> PresentedPolicyState<'v> {
    PresentedPolicyState {
        state: name,
        mode: state.mode.unwrap_or(StateMode::Parallel),
        max_active_ms: data.max_active_ms,
        retry: present_retry(data.retry.as_ref(), Some(state)),
        retention: present_retention(defaults, data.retention, Some(state)),
        governance: state.governance.as_ref(),
    }
}
