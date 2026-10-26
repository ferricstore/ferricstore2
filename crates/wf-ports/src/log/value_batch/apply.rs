//! Actual value-profile Fresh dispatch with retained original-slot ordering.

use std::mem::size_of;

use wf_domain::policy::definition::StateMode;
use wf_domain::workflow::indexed::{OrderingMetadata, TouchedIndexedGroupView};

use crate::log::record_batch::{admit, independent, product, sum};
use crate::log::{
    CodecError, FreshRecordBatch, RecordBatchApplyError, RecordBatchApplyPlan,
    decide_fresh_atomic_start_touched, decide_fresh_public_mutation_touched,
    decide_fresh_record_batch_touched,
};

use super::response::effective_profile;

/// The same accepted plan plus existing original-slot ordering for contained6.
pub type WorkflowValueDecision<'current> = (
    RecordBatchApplyPlan<'current>,
    Vec<Option<(OrderingMetadata, StateMode)>>,
);

/// Consumes actual intent8 Fresh inputs once against coherent touched authority.
///
/// Contained6 moves the kernel's existing ordering; other modes return no slots.
/// The retained vector is admitted in addition to declared external residency.
/// Callers must fund its continued lifetime after this returned owner escapes.
/// # Errors
/// Rejects crossed profiles, current authority, retention or prospective workspace.
pub fn decide_fresh_workflow_value_batch_touched<'current>(
    fresh: FreshRecordBatch<'_>,
    view: TouchedIndexedGroupView<'_, 'current>,
    additional_live_bytes: usize,
    outcome_ttl_ms: u64,
) -> Result<WorkflowValueDecision<'current>, RecordBatchApplyError> {
    if fresh.decoded.request.intent.version() != 8 {
        return Err(CodecError::UnsupportedVersion.into());
    }
    let mode = effective_profile(&fresh.decoded)?;
    let count = fresh
        .decoded
        .request
        .operation
        .mutation_items()
        .ok_or(CodecError::UnsupportedTag)?
        .len();
    let slots = if mode == 6 { count } else { 0 };
    let ordering = sum(&[
        size_of::<Vec<Option<(OrderingMetadata, StateMode)>>>(),
        product(slots, size_of::<Option<(OrderingMetadata, StateMode)>>())?,
    ])?;
    let live = sum(&[additional_live_bytes, ordering])?;
    admit(live, fresh.workspace_limit)?;
    if mode == 6 {
        return independent::decide_with_ordering(fresh, view, live, outcome_ttl_ms);
    }
    let plan = match mode {
        5 => decide_fresh_record_batch_touched(fresh, view, live, outcome_ttl_ms)?,
        7 => decide_fresh_atomic_start_touched(fresh, view, live, outcome_ttl_ms)?,
        1 => decide_fresh_public_mutation_touched(fresh, view, live, outcome_ttl_ms)?,
        _ => return Err(CodecError::UnsupportedVersion.into()),
    };
    Ok((plan, Vec::new()))
}
