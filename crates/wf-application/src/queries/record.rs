//! Strong current-record orchestration; accepted quorum adapters remain external.
mod budget;
mod dispatch;
mod model;
pub use model::{
    StrongWorkflowRecordError, StrongWorkflowRecordLimits, StrongWorkflowRecordRequest,
    StrongWorkflowRecordResult,
};
use wf_domain::value::OriginalValueContext;
use wf_ports::state::StrongWorkflowRecordReadPort;

/// Query the exact authorized current record at the requested scoped minimum.
/// # Errors
/// Keep authority, consistency, storage, context and resource failures distinct.
pub fn query_strong_workflow_record<P: StrongWorkflowRecordReadPort>(
    port: &mut P,
    request: StrongWorkflowRecordRequest<'_>,
    context: &OriginalValueContext<'_>,
    limits: StrongWorkflowRecordLimits,
) -> Result<StrongWorkflowRecordResult, StrongWorkflowRecordError> {
    let mut budget = budget::Budget::new(request, limits)?;
    dispatch::minimum(request)?;
    let physical = budget.route(request)?;
    let scope = budget.scope(request.scope)?;
    let mut read = limits.read;
    read.record.maximum_work = read.record.maximum_work.min(budget.remaining()?);
    read.record.maximum_temporary_bytes = read.record.maximum_temporary_bytes.min(budget.spare()?);
    let observation = port
        .read_strong_workflow_record(request.scope, &physical, request.id, request.after, read)
        .map_err(StrongWorkflowRecordError::Read)?;
    dispatch::observation(&observation, request, &physical, &mut budget)?;
    let mut selected = limits.preparation;
    selected.query.maximum_work_units = budget.remaining()?;
    selected.query.maximum_temporary_bytes = budget.spare()?;
    let native = wf_ports::projection::query_row::prepare_native_query_snapshot(
        observation.snapshot,
        context,
        selected,
    )
    .map_err(dispatch::preparation)?;
    dispatch::finish(native, scope, request, observation.now, &mut budget)
}
