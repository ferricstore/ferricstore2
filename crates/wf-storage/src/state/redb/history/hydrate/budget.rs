//! Checked composition cohorts, separate from all reached component accounts.

use std::mem::size_of;

use wf_domain::history::values::HistoryValueEntry;
use wf_ports::blob::WorkflowValueSizedReadBatch;
use wf_ports::state::StoredWorkflowHistoryRow;
use wf_ports::state::{
    StateTransactionError, WorkflowHistoryHydrationLimits, WorkflowHistoryHydrationRequest,
    WorkflowHistoryValuePage,
};

pub(super) use super::super::super::values::budget::{ValueCharge, product, total};
use super::references::HistoryReferences;

pub(super) fn fixed() -> Result<usize, StateTransactionError> {
    total(
        size_of::<WorkflowHistoryValuePage>(),
        [
            size_of::<WorkflowHistoryHydrationLimits>(),
            size_of::<WorkflowHistoryHydrationRequest<'_>>(),
            size_of::<HistoryReferences<'_>>(),
            size_of::<WorkflowValueSizedReadBatch>(),
            size_of::<Vec<HistoryValueEntry<'_, StoredWorkflowHistoryRow>>>(),
            size_of::<ValueCharge>(),
            size_of::<[usize; 16]>(),
            size_of::<[u128; 2]>(),
        ],
    )
}

pub(super) fn start(
    limits: WorkflowHistoryHydrationLimits,
) -> Result<ValueCharge, StateTransactionError> {
    let mut charge = ValueCharge::new(1, 1, limits.maximum_work, limits.maximum_temporary_bytes)?;
    stage(
        &mut charge,
        limits.history.maximum_work,
        0,
        limits.history.maximum_temporary_bytes,
    )?;
    Ok(charge)
}

pub(super) fn stage(
    charge: &mut ValueCharge,
    work: usize,
    held: usize,
    maximum: usize,
) -> Result<(), StateTransactionError> {
    charge.work(work)?;
    temporary(charge, held, maximum)
}

pub(super) fn temporary(
    charge: &mut ValueCharge,
    held: usize,
    extra: usize,
) -> Result<(), StateTransactionError> {
    charge.temporary(total(fixed()?, [held, extra])?)
}

pub(super) fn reserve<T>(
    charge: &mut ValueCharge,
    count: usize,
    held: usize,
) -> Result<Vec<T>, StateTransactionError> {
    charge.reserve(count, total(fixed()?, [held])?)
}
