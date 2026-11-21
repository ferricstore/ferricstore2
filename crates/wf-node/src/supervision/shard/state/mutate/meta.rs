//! The fields beyond the record after an accepted mutation (ADR-014): its
//! attributes, state metadata, values and kept versions, and the terminal
//! retention it starts.

use wf_application::commands::flow::meta;
use wf_application::commands::flow::mutate::{Change, Mutation};
use wf_application::commands::flow::record::{
    FlowMeta, RETAINED_ERROR, RETAINED_PAYLOAD, RETAINED_RESULT,
};
use wf_application::commands::queue::{Partition, ServiceError};
use wf_domain::workflow::{WorkflowRecord, WorkflowStatus};

/// The state whose metadata a mutation updates.
fn meta_state<'a>(mutation: &'a Mutation, record: &'a WorkflowRecord) -> &'a [u8] {
    match &mutation.change {
        Change::Transition { to, .. } | Change::StepContinue { to, .. } => to,
        Change::Complete { .. } => b"completed",
        Change::Fail { .. } => b"failed",
        Change::Cancel { .. } => b"cancelled",
        Change::Retry { .. } | Change::ExtendLease { .. } => {
            record.run_state().unwrap_or(record.state())
        }
    }
}

/// The fields beyond the record after an accepted mutation.
pub(super) fn updated_meta(
    (mutation, partition): (&Mutation, &Partition),
    current: &FlowMeta,
    (previous, record): (&WorkflowRecord, &WorkflowRecord),
    wall: u64,
) -> Result<FlowMeta, ServiceError> {
    let mut next = current.clone();
    let version = record.version();
    next.attributes = meta::update_attributes(
        &current.attributes,
        &mutation.attributes_merge,
        &mutation.attributes_delete,
    )?;
    next.state_meta = meta::merge_state_meta(
        &current.state_meta,
        meta_state(mutation, record),
        &mutation.state_meta,
    )?;
    super::super::value::write_values(&mut next, &mutation.values, &mutation.override_values)?;
    super::super::value::drop_values(&mut next, &mutation.drop_values);
    super::super::value::attach(&mut next, &mutation.value_refs, partition)?;
    let (payload, result, error, ttl) = match &mutation.change {
        Change::Transition { payload, .. } | Change::StepContinue { payload, .. } => {
            (payload.is_some(), false, false, None)
        }
        Change::Retry { payload, error, .. } => (payload.is_some(), false, error.is_some(), None),
        Change::Complete {
            payload,
            result,
            ttl_ms,
        } => (payload.is_some(), result.is_some(), false, *ttl_ms),
        Change::Fail {
            payload,
            error,
            ttl_ms,
        } => (payload.is_some(), false, error.is_some(), *ttl_ms),
        Change::Cancel { reason, ttl_ms, .. } => (false, false, reason.is_some(), *ttl_ms),
        Change::ExtendLease { .. } => (false, false, false, None),
    };
    // Replaced values stay readable under their versions (ADR-016 phase
    // C).
    if payload {
        if let Some(old) = previous.payload() {
            next.retain((RETAINED_PAYLOAD, b"", current.payload_version), old);
        }
        next.payload_version = version;
    }
    if result {
        if let Some(old) = previous.result() {
            next.retain((RETAINED_RESULT, b"", current.result_version), old);
        }
        next.result_version = version;
    }
    if error {
        if let Some(old) = previous.error() {
            next.retain((RETAINED_ERROR, b"", current.error_version), old);
        }
        next.error_version = version;
    }
    if matches!(
        record.status(),
        WorkflowStatus::Completed | WorkflowStatus::Failed | WorkflowStatus::Cancelled
    ) {
        next.ttl_ms = ttl;
        // Retention starts at the command's time, never before the wall
        // clock (Elixir's `flow_stamp_terminal_retention`).
        let start = record.updated_at().get().max(wall);
        next.terminal_until = Some(start.saturating_add(ttl.unwrap_or(current.retention_ttl_ms)));
    }
    Ok(next)
}
