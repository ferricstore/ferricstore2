//! Explicit intent22 admission and same-plan metadata ownership.
use super::{ValuePublicationPlan, storage_error};
use super::{WorkflowValueTransactionError, WorkflowValueTransactionLimits};
use crate::state::RedbStateStore;
use crate::state::redb::values::{
    budget::{ValueCharge, product, total},
    metadata as storage,
};
use crate::transaction::RecordBatchTransactionOutcome;
use std::{mem::size_of, sync::Arc};
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::StateTransactionError;
use wf_types::version::LogIdentity;

pub(crate) struct MetadataPublication {
    pub(crate) steps: Vec<Arc<Vec<u8>>>,
}

/// Applies explicit metadata caller operands with core/value publication atomically.
/// # Errors
/// Rejects unsupported authority, missing birth captures or unproved durable commit.
pub fn apply_workflow_metadata_batch_transaction(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: WorkflowValueTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, WorkflowValueTransactionError> {
    super::driver::apply(store, scope, encoded, log, limits, 22)
}

pub(super) fn held_bytes(
    updates: Option<&[Option<Vec<u8>>]>,
    charge: &mut ValueCharge,
) -> Result<usize, WorkflowValueTransactionError> {
    let Some(updates) = updates else {
        return Ok(0);
    };
    charge
        .work(product(updates.len(), 128).map_err(storage_error)?)
        .map_err(storage_error)?;
    let mut size = total(
        4096,
        [product(updates.len(), size_of::<Option<Vec<u8>>>()).map_err(storage_error)?],
    )
    .map_err(storage_error)?;
    for value in updates.iter().flatten() {
        size = total(size, [value.len()]).map_err(storage_error)?;
    }
    Ok(size)
}

pub(super) fn prepare(
    tx: &crate::state::RedbStateTransaction<'_>,
    scope: &RequestScope,
    publication: &mut ValuePublicationPlan<'_>,
    mut updates: Option<Vec<Option<Vec<u8>>>>,
    maximum_queries: usize,
) -> Result<(), WorkflowValueTransactionError> {
    let charge = &mut publication.charge;
    charge.helper().map_err(storage_error)?;
    let held = held_bytes(updates.as_deref(), charge)?;
    let live = total(
        charge.peak(),
        [
            held,
            size_of::<MetadataPublication>(),
            size_of::<Vec<Option<Arc<Vec<u8>>>>>(),
            4096,
        ],
    )
    .map_err(storage_error)?;
    let mut retained = 0;
    let mut steps: Vec<Arc<Vec<u8>>> = charge
        .reserve(publication.steps.len(), live)
        .map_err(storage_error)?;
    let live = total(
        live,
        [product(publication.steps.len(), size_of::<Arc<Vec<u8>>>()).map_err(storage_error)?],
    )
    .map_err(storage_error)?;
    charge
        .work(product(publication.steps.len(), 128).map_err(storage_error)?)
        .map_err(storage_error)?;
    let slots = publication
        .steps
        .iter()
        .map(|step| step.query_index)
        .max()
        .map_or(Some(0), |n| n.checked_add(1))
        .ok_or_else(|| storage_error(StateTransactionError::Limit))?;
    if slots > maximum_queries {
        return Err(WorkflowValueTransactionError::InvalidInput);
    }
    charge
        .work(product(slots, 128).map_err(storage_error)?)
        .map_err(storage_error)?;
    let mut cache: Vec<Option<Arc<Vec<u8>>>> =
        charge.reserve(slots, live).map_err(storage_error)?;
    cache.resize(slots, None);
    let live = total(
        live,
        [product(slots, size_of::<Option<Arc<Vec<u8>>>>()).map_err(storage_error)?],
    )
    .map_err(storage_error)?;
    for step in &publication.steps {
        charge.candidate().map_err(storage_error)?;
        let source = usize::try_from(step.source_index)
            .map_err(|_| WorkflowValueTransactionError::InvalidInput)?;
        let supplied = if let Some(inputs) = &mut updates {
            inputs
                .get_mut(source)
                .ok_or(WorkflowValueTransactionError::InvalidInput)?
                .take()
        } else {
            None
        };
        let previous = cache
            .get(step.query_index)
            .ok_or_else(|| storage_error(StateTransactionError::Corrupt))?
            .as_ref();
        let chosen = choose(
            tx,
            scope,
            step,
            supplied,
            previous,
            (
                charge,
                total(live, [retained]).map_err(storage_error)?,
                &mut retained,
            ),
        )?;
        *cache
            .get_mut(step.query_index)
            .ok_or_else(|| storage_error(StateTransactionError::Corrupt))? =
            Some(Arc::clone(&chosen));
        steps.push(chosen);
    }
    publication.metadata = Some(MetadataPublication { steps });
    Ok(())
}

fn new_owner(
    bytes: Vec<u8>,
    charge: &mut ValueCharge,
    live: usize,
    retained: &mut usize,
) -> Result<Arc<Vec<u8>>, WorkflowValueTransactionError> {
    let owner = total(bytes.len(), [size_of::<Vec<u8>>(), 2 * size_of::<usize>()])
        .map_err(storage_error)?;
    *retained = total(*retained, [owner]).map_err(storage_error)?;
    charge
        .temporary(total(live, [*retained]).map_err(storage_error)?)
        .map_err(storage_error)?;
    Ok(Arc::new(bytes))
}

fn choose(
    tx: &crate::state::RedbStateTransaction<'_>,
    scope: &RequestScope,
    step: &super::ValueStep<'_>,
    supplied: Option<Vec<u8>>,
    previous: Option<&Arc<Vec<u8>>>,
    (charge, live, retained): (&mut ValueCharge, usize, &mut usize),
) -> Result<Arc<Vec<u8>>, WorkflowValueTransactionError> {
    if let Some(bytes) = supplied {
        return new_owner(bytes, charge, live, retained);
    }
    if let Some(previous) = previous {
        return Ok(Arc::clone(previous));
    }
    if step
        .plan
        .plan()
        .history()
        .is_some_and(|event| event.kind == wf_domain::history::EventKind::Created)
    {
        return Err(WorkflowValueTransactionError::InvalidInput);
    }
    let write = step
        .plan
        .plan()
        .write()
        .ok_or(WorkflowValueTransactionError::InvalidInput)?;
    let data = storage::load_current(
        tx,
        scope,
        (step.plan.partition(), write.record.id()),
        step.binding.lifecycle.incarnation,
        charge,
        live,
    )
    .map_err(storage_error)?;
    new_owner(data, charge, live, retained)
}
