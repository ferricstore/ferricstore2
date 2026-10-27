//! Coherent physical history acquisition and source-compatible value hydration.

use std::mem::size_of;

use wf_domain::history::values::{
    HistoryValueEntry, HistoryValuePlan, HistoryValueReadBatch, HistoryValueReturn,
    apply_history_value_results, prepare_history_values,
};
use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::{WorkflowValueBindingKnowledge, convert_history_value_reads};
use wf_ports::clock::WorkflowValueReadClock;
use wf_ports::state::{
    StateTransactionError, StoredWorkflowHistoryRow, WorkflowHistoryHydrationError as Error,
    WorkflowHistoryHydrationLimits, WorkflowHistoryHydrationRequest, WorkflowHistoryPage,
    WorkflowHistoryValuePage,
};
use wf_types::identity::{PartitionId, RecordId};

use super::super::{RedbStateStore, values::read::read_captured};
use super::{
    page,
    read::{self, HistoryReadContext},
};
use budget::{ValueCharge, product, total};
use references::HistoryReferences;

mod budget;
mod references;

#[cfg(test)]
mod tests;

impl super::super::RedbStateStore {
    /// Captures and hydrates actual history candidates on one read snapshot.
    ///
    /// Caller still applies lifecycle visibility and an authoritative read barrier.
    /// # Errors
    /// Preserves state, clock, conversion and projection refusals explicitly.
    pub fn read_workflow_history_page_with_values<C: WorkflowValueReadClock>(
        &self,
        clock: &mut C,
        scope: &RequestScope,
        partition: &PartitionId,
        id: &RecordId,
        request: WorkflowHistoryHydrationRequest<'_>,
        limits: WorkflowHistoryHydrationLimits,
    ) -> Result<WorkflowHistoryValuePage, Error> {
        match capture(self, clock, scope, partition, id, request, limits) {
            Err(Error::State(error)) => Err(Error::State(self.failed(error))),
            result => result,
        }
    }
}

fn capture<C: WorkflowValueReadClock>(
    store: &RedbStateStore,
    clock: &mut C,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    request: WorkflowHistoryHydrationRequest<'_>,
    limits: WorkflowHistoryHydrationLimits,
) -> Result<WorkflowHistoryValuePage, Error> {
    store.ready()?;
    if scope.group() != &store.group
        || scope.recovery() != &store.recovery
        || request.query.limit == 0
    {
        return Err(StateTransactionError::InvalidChanges.into());
    }
    let mut charge = budget::start(limits)?;
    let mut context = read::open(store, scope, request.query, limits.history)?;
    let rows = context.open_rows()?;
    let mut failed = false;
    let result = (|| {
        let page = page::capture(&mut context, &rows, scope, partition, id, request.query)?;
        hydrate(
            &context,
            &rows,
            scope,
            page,
            request,
            limits,
            (clock, &mut charge, &mut failed),
        )
    })();
    read::close(context, rows);
    if failed {
        store.failed(StateTransactionError::Unavailable);
    }
    result
}

fn hydrate<C: WorkflowValueReadClock>(
    context: &HistoryReadContext,
    rows: &redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
    history: WorkflowHistoryPage,
    request: WorkflowHistoryHydrationRequest<'_>,
    limits: WorkflowHistoryHydrationLimits,
    (clock, charge, failed): (&mut C, &mut ValueCharge, &mut bool),
) -> Result<WorkflowHistoryValuePage, Error> {
    if matches!(request.value_return, HistoryValueReturn::Disabled) || history.entries.is_empty() {
        return Ok(WorkflowHistoryValuePage {
            history,
            writes: Vec::new(),
            projection_work_units: 0,
            projection_requested_bytes: 0,
        });
    }
    if context.facts.is_none() {
        return Err(StateTransactionError::UnsupportedLayout.into());
    }
    let WorkflowValueBindingKnowledge::Known(Some(current)) = &history.current_values else {
        return Err(StateTransactionError::Corrupt.into());
    };
    let incarnation = current.lifecycle.incarnation;
    let held = context.live(0)?;
    budget::stage(
        charge,
        limits.projection.maximum_work_units,
        held,
        limits.projection.maximum_requested_bytes,
    )?;
    let (writes, projection_work_units, projection_requested_bytes) = {
        let references = HistoryReferences::capture(&history, charge, held)?;
        let views = references.views(&history, charge, held)?;
        let live = total(
            held,
            [
                references.live,
                product(
                    views.capacity(),
                    size_of::<HistoryValueEntry<'_, StoredWorkflowHistoryRow>>(),
                )?,
            ],
        )?;
        let plan = prepare_history_values(&views, request.value_return, limits.projection)?;
        let batch = acquire(
            context,
            rows,
            scope,
            &plan,
            limits,
            incarnation,
            (clock, charge, failed, live),
        )?;
        budget::temporary(charge, live, limits.projection.maximum_requested_bytes)?;
        let overlay = apply_history_value_results(plan, batch, &request.decode_context)?;
        (
            overlay.writes,
            overlay.charged_work,
            overlay.charged_requested_bytes,
        )
    };
    Ok(WorkflowHistoryValuePage {
        history,
        writes,
        projection_work_units,
        projection_requested_bytes,
    })
}

fn acquire<C: WorkflowValueReadClock>(
    context: &HistoryReadContext,
    rows: &redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
    plan: &HistoryValuePlan<'_, StoredWorkflowHistoryRow>,
    limits: WorkflowHistoryHydrationLimits,
    incarnation: u128,
    (clock, charge, failed, held): (&mut C, &mut ValueCharge, &mut bool, usize),
) -> Result<Option<HistoryValueReadBatch<'static>>, Error> {
    let references = plan.references();
    if references.is_empty() {
        return Ok(None);
    }
    if references.len() > limits.values.maximum_references {
        return Err(StateTransactionError::Limit.into());
    }
    let live = total(held, [plan.charged_requested_bytes()])?;
    budget::stage(
        charge,
        limits.values.maximum_work,
        live,
        limits.values.maximum_temporary_bytes,
    )?;
    let mut values = ValueCharge::new(
        limits.values.maximum_references.max(1),
        limits.values.maximum_stored_bytes,
        limits.values.maximum_work,
        limits.values.maximum_temporary_bytes,
    )?;
    values.candidate()?;
    let time = clock.read_snapshot().map_err(Error::Clock)?;
    let facts = context.facts.ok_or(StateTransactionError::Corrupt)?;
    let threshold = plan
        .read_threshold()
        .ok_or(StateTransactionError::Corrupt)?;
    let batch = read_captured(
        rows,
        &mut values,
        (
            context.limits,
            facts.alias_format,
            limits.values,
            incarnation,
        ),
        scope,
        references,
        time,
        (threshold, 0, failed),
    )?;
    budget::stage(
        charge,
        limits.conversion.maximum_work,
        live,
        limits.conversion.maximum_temporary_bytes,
    )?;
    Ok(Some(convert_history_value_reads(
        batch,
        references.len(),
        limits.conversion,
    )?))
}
