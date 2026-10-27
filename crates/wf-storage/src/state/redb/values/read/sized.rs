//! Authenticated omission boundary; R0 deliberately refuses every input.

use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::{WorkflowValueReadLimits, WorkflowValueSizedReadBatch};
use wf_ports::clock::WorkflowValueReadClock;
use wf_ports::state::StateTransactionError as Error;

use super::RedbStateStore;

pub(in super::super) fn read_sized<C: WorkflowValueReadClock>(
    _store: &RedbStateStore,
    _clock: &mut C,
    _scope: &RequestScope,
    _references: &[&[u8]],
    _omission_threshold: usize,
    _limits: WorkflowValueReadLimits,
) -> Result<WorkflowValueSizedReadBatch, Error> {
    Err(Error::UnsupportedLayout)
}

#[cfg(test)]
mod tests;
