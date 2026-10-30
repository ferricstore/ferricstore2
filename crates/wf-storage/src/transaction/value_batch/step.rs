//! Reached birth capture and source-presence Keep/Clear/Replace transitions.

use wf_domain::history::EventKind;
use wf_domain::policy::complete::{
    PolicySchemaCharge, StoredLifecycleCaptureError, StoredLifecycleCaptureInput,
    StoredPolicyValue, capture_stored_creation_lifecycle,
};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::active::{LifecycleOverrides, MaxActiveOverride};
use wf_domain::workflow::batch::ScopedPlan;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_ports::blob::WorkflowValueLifecycle;
use wf_ports::consensus::WorkflowValueItemInput;
use wf_ports::state::StateTransactionError;
use wf_types::version::TimestampMillis;

use crate::state::redb::values::budget::{ValueCharge, product, total};
use crate::state::value::ValueBindingData;

use super::prepare::ValueInputs;
use super::{
    WorkflowValueTransactionError as Error, WorkflowValueTransactionLimits, record_error,
    storage_error,
};

pub(super) fn birth(
    scoped: &ScopedPlan,
    ordering: OrderingMetadata,
    input: WorkflowValueItemInput,
    inputs: &ValueInputs,
    policies: &[Option<StoredPolicyValue>],
    schema: &mut PolicySchemaCharge,
    resources: (&mut ValueCharge, &WorkflowValueTransactionLimits, usize),
) -> Result<ValueBindingData, Error> {
    let (charge, limits, live) = resources;
    charge.helper().map_err(storage_error)?;
    let record = &scoped
        .plan()
        .write()
        .ok_or_else(|| storage_error(StateTransactionError::Corrupt))?
        .record;
    let bound = inputs
        .policies
        .get(input.policy_index as usize)
        .ok_or(Error::InvalidInput)?;
    let policy = policies
        .get(bound.loaded_index)
        .ok_or_else(|| storage_error(StateTransactionError::Corrupt))?
        .as_ref();
    let scratch = match policy {
        Some(StoredPolicyValue::Original(raw)) => {
            total(65_536, [product(raw.len(), 256).map_err(storage_error)?])
                .map_err(storage_error)?
        }
        _ => 0,
    };
    charge
        .temporary(total(live, [scratch]).map_err(storage_error)?)
        .map_err(storage_error)?;
    let overrides = input.birth.unwrap_or(LifecycleOverrides {
        active: MaxActiveOverride::Inherit,
        retention: RetentionOverride::default(),
        history_hot_max_events: None,
    });
    let birth = capture_stored_creation_lifecycle(
        policy,
        StoredLifecycleCaptureInput {
            workflow_type: record.workflow_type(),
            initial_state: record.run_state().unwrap_or(record.state()),
            overrides,
            defaults: inputs.defaults.lifecycle,
        },
        limits.record_batch.schema,
        schema,
        scratch,
    )
    .map_err(|error| match error {
        StoredLifecycleCaptureError::Policy(error) => record_error(error.into()),
        StoredLifecycleCaptureError::Capture(_) => Error::InvalidInput,
    })?;
    Ok(ValueBindingData {
        versions: [None; 3],
        lifecycle: WorkflowValueLifecycle {
            incarnation: ordering.state_enter_seq,
            birth,
            effective_ttl_ms: Some(birth.retention_ttl_ms()),
            terminal_deadline: None,
            configuration_digest: inputs.defaults.configuration_digest,
            policy_profile: bound.profile,
            policy_generation: bound.generation,
            policy_digest: bound.digest,
        },
    })
}

pub(super) fn advance(
    scoped: &ScopedPlan,
    input: WorkflowValueItemInput,
    mut binding: ValueBindingData,
    supplied: [bool; 3],
    execution: TimestampMillis,
    charge: &mut ValueCharge,
) -> Result<ValueBindingData, Error> {
    charge.helper().map_err(storage_error)?;
    let history = scoped
        .plan()
        .history()
        .ok_or_else(|| storage_error(StateTransactionError::Corrupt))?;
    let record = &scoped
        .plan()
        .write()
        .ok_or_else(|| storage_error(StateTransactionError::Corrupt))?
        .record;
    for (index, present) in supplied.into_iter().enumerate() {
        charge.candidate().map_err(storage_error)?;
        if present {
            binding.versions[index] = Some(record.version());
        } else if (index == 1 && history.kind == EventKind::Completed)
            || (index == 2
                && matches!(
                    history.kind,
                    EventKind::Failed | EventKind::Retry | EventKind::Cancelled
                ))
        {
            binding.versions[index] = None;
        }
    }
    if history.kind == EventKind::StepContinued {
        binding.lifecycle.effective_ttl_ms = None;
        binding.lifecycle.terminal_deadline = None;
    }
    if matches!(record.state(), b"completed" | b"failed" | b"cancelled") {
        if let Some(ttl) = input.terminal_ttl_ms
            && matches!(
                history.kind,
                EventKind::Completed | EventKind::Failed | EventKind::Cancelled
            )
        {
            binding.lifecycle.effective_ttl_ms = u64::try_from(ttl).ok().filter(|ttl| *ttl != 0);
        }
        let base = input.requested_at.unwrap_or(execution).max(execution).get();
        binding.lifecycle.terminal_deadline = binding
            .lifecycle
            .effective_ttl_ms
            .map(|ttl| TimestampMillis::new(base.saturating_add(ttl).min(9_007_199_254_740_991)));
    }
    Ok(binding)
}

pub(super) fn live(inputs: &ValueInputs) -> Result<usize, StateTransactionError> {
    use std::mem::size_of;
    total(
        size_of::<ValueInputs>(),
        [
            size_of::<PolicySchemaCharge>(),
            product(inputs.inputs.len(), size_of::<WorkflowValueItemInput>())?,
            product(
                inputs.policies.len(),
                size_of::<super::prepare::BoundValuePolicy>(),
            )?,
        ],
    )
}
