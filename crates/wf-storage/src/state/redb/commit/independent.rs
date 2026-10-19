//! Original versions once, final cores once, and every successful history event.

use super::super::{publication, store::RedbStateTransaction};
use wf_domain::workflow::batch::ScopedPlan;
use wf_domain::workflow::indexed::IndependentIndexedPlan;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::StateTransactionError;

pub(super) fn apply(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    business: &IndependentIndexedPlan<'_>,
) -> Result<(), StateTransactionError> {
    for (index, original) in business.final_indices().zip(business.originals()) {
        let item = final_item(business, index)?;
        let write = item
            .plan()
            .write()
            .ok_or(StateTransactionError::InvalidChanges)?;
        publication::validate_version(
            tx,
            scope,
            item.partition(),
            write.record.id(),
            original.map(|row| row.record.version()),
        )?;
    }
    let lanes = publication::affected_lanes(
        tx,
        business.final_indices().len(),
        business
            .deltas()
            .map(|delta| delta.map_err(|_| StateTransactionError::InvalidChanges)),
    )?;
    for lane in &lanes {
        publication::head_due(tx, scope, *lane, false)?;
    }
    for delta in business.deltas() {
        publication::index_delta(
            tx,
            scope,
            &delta.map_err(|_| StateTransactionError::InvalidChanges)?,
        )?;
    }
    for (index, ordering) in business.final_indices().zip(business.metadata()) {
        let item = final_item(business, index)?;
        let write = item
            .plan()
            .write()
            .ok_or(StateTransactionError::InvalidChanges)?;
        publication::write_record(tx, scope, item.partition(), &write.record, ordering)?;
    }
    for item in business.history_plans() {
        let write = item
            .plan()
            .write()
            .ok_or(StateTransactionError::InvalidChanges)?;
        let history = item
            .plan()
            .history()
            .ok_or(StateTransactionError::InvalidChanges)?;
        publication::write_history(tx, scope, item.partition(), write.record.id(), history)?;
    }
    for lane in lanes {
        publication::head_due(tx, scope, lane, true)?;
    }
    Ok(())
}

fn final_item<'p>(
    business: &'p IndependentIndexedPlan<'_>,
    index: usize,
) -> Result<&'p ScopedPlan, StateTransactionError> {
    business
        .results()
        .get(index)
        .and_then(|result| result.as_ref().ok())
        .ok_or(StateTransactionError::InvalidChanges)
}
