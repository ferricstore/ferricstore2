//! Scoped current record and regular payload/result/error hydration composition.
mod budget;
mod hydrate;
mod model;
mod reference;
pub use model::{
    StrongWorkflowRecordValueError, StrongWorkflowRecordValueLimits,
    StrongWorkflowRecordValueRequest, StrongWorkflowRecordValueResult,
};
use wf_domain::value::OriginalValueContext;
use wf_ports::{blob::AuthorizedWorkflowValueReadPort, state::StrongWorkflowRecordReadPort};

/// Query and hydrate exact regular refs, preserving source field-write semantics.
/// # Errors
/// Preserve authority, read, projection, context and whole-call resource failures.
pub fn query_strong_workflow_record_with_values<
    P: StrongWorkflowRecordReadPort + AuthorizedWorkflowValueReadPort,
>(
    port: &mut P,
    request: StrongWorkflowRecordValueRequest<'_>,
    context: &OriginalValueContext<'_>,
    limits: StrongWorkflowRecordValueLimits,
) -> Result<StrongWorkflowRecordValueResult, StrongWorkflowRecordValueError> {
    let mut b = budget::Budget::new(&limits)?;
    let mut record_limits = limits.record;
    record_limits.preparation.query.maximum_work_units = record_limits
        .preparation
        .query
        .maximum_work_units
        .min(b.remaining()?);
    record_limits.preparation.query.maximum_temporary_bytes = record_limits
        .preparation
        .query
        .maximum_temporary_bytes
        .min(b.spare()?);
    record_limits.preparation.query.maximum_output_bytes = record_limits
        .preparation
        .query
        .maximum_output_bytes
        .min(limits.maximum_output_bytes);
    let record = super::query_strong_workflow_record(port, request.record, context, record_limits)
        .map_err(StrongWorkflowRecordValueError::Query)?;
    b.peer(record.work_units(), record.temporary_bytes())?;
    b.admit(record.owned_bytes())?;
    let writes = if request.value_return.enabled() && record.record().is_some() {
        hydrate::writes(
            port,
            &record,
            request.value_return.value_return(),
            context,
            &mut b,
        )?
    } else {
        Vec::new()
    };
    b.finish(record, writes)
}
