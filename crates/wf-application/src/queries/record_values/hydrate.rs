//! One continuous existing collector/projector over the exact observed native base.
use super::super::StrongWorkflowRecordResult;
use super::{StrongWorkflowRecordValueError as E, budget::Budget, reference};
use wf_domain::{
    history::values::{
        HistoryValueEntry, HistoryValueReference as R, HistoryValueReturn, HistoryValueWrite,
        apply_history_value_results, prepare_history_values,
    },
    value::OriginalValueContext,
};
use wf_ports::blob::{AuthorizedWorkflowValueReadPort, convert_history_value_reads};

pub(super) fn writes<P: AuthorizedWorkflowValueReadPort>(
    port: &mut P,
    base: &StrongWorkflowRecordResult,
    value_return: HistoryValueReturn,
    context: &OriginalValueContext<'_>,
    b: &mut Budget,
) -> Result<Vec<HistoryValueWrite>, E> {
    let Some(record) = base.record() else {
        return Ok(Vec::new());
    };
    let refs = reference::collect(record, b)?;
    if refs.iter().flatten().all(|bytes| bytes.is_empty()) {
        return Ok(Vec::new());
    }
    let fields = std::array::from_fn(|index| refs[index].as_deref().map_or(R::Nil, R::Binary));
    let entries = [HistoryValueEntry {
        source: record,
        references: fields,
    }];
    let limits = b.limits.projection;
    // Fund the unchanged shared account once, before prepare and its allocations.
    b.fund(limits.maximum_work_units, limits.maximum_requested_bytes)?;
    let plan = prepare_history_values(&entries, value_return, limits).map_err(E::Projection)?;
    let batch = if plan.references().is_empty() {
        None
    } else {
        let read = b.limits.values;
        b.fund(read.maximum_work, read.maximum_temporary_bytes)?;
        let captured = port
            .read_authorized_workflow_values(
                base.scope(),
                plan.references(),
                plan.read_threshold().ok_or(E::Limit)?,
                read,
            )
            .map_err(E::Read)?;
        let conversion = b.limits.conversion;
        b.fund(conversion.maximum_work, conversion.maximum_temporary_bytes)?;
        Some(
            convert_history_value_reads(captured, plan.references().len(), conversion)
                .map_err(E::Conversion)?,
        )
    };
    let overlay = apply_history_value_results(plan, batch, context).map_err(E::Projection)?;
    Ok(overlay.writes)
}
