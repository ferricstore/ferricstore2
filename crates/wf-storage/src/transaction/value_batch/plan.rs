//! Publication borrows the accepted plan after its complete owner is assembled.

use std::mem::size_of;

use wf_domain::history::EventKind;
use wf_domain::policy::complete::{PolicySchemaCharge, StoredPolicyValue};
use wf_domain::policy::definition::StateMode;
use wf_domain::workflow::batch::ScopedPlan;
use wf_domain::workflow::control::PreparedDigest;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_ports::blob::RecordBatchValueEffect;
use wf_ports::state::{GroupApplyChanges, StateTransactionError};
use wf_types::identity::{PartitionId, RecordId};

use crate::state::redb::values::budget::{ValueCharge, product, total};
use crate::state::value::ValueBindingData;

use super::prepare::ValueInputs;
use super::{
    WorkflowValueTransactionError as Error, WorkflowValueTransactionLimits, step, storage_error,
};

pub(crate) struct ValueStep<'plan> {
    pub(crate) source_index: u32,
    pub(crate) query_index: usize,
    pub(crate) plan: &'plan ScopedPlan,
    pub(crate) ordering: OrderingMetadata,
    pub(crate) binding: ValueBindingData,
    pub(crate) supplied: [bool; 3],
    pub(crate) final_write: bool,
}

pub(crate) struct ValuePublicationPlan<'plan> {
    pub(crate) steps: Vec<ValueStep<'plan>>,
    pub(crate) prepared: PreparedDigest,
    pub(crate) mode: u8,
    pub(crate) charge: ValueCharge,
}

pub(super) fn build<'plan>(
    changes: &'plan GroupApplyChanges<'_, '_>,
    inputs: &ValueInputs,
    originals: (&[(PartitionId, RecordId)], &[Option<ValueBindingData>]),
    policies: &mut (&[Option<StoredPolicyValue>], &mut PolicySchemaCharge),
    ordering: &[Option<(OrderingMetadata, StateMode)>],
    limits: &WorkflowValueTransactionLimits,
    mut charge: ValueCharge,
) -> Result<ValuePublicationPlan<'plan>, Error> {
    charge.helper().map_err(storage_error)?;
    let effects = changes
        .plan()
        .value_effects()
        .map_or(&[][..], wf_ports::blob::RecordBatchValueEffects::steps);
    let live = total(
        step::live(inputs).map_err(storage_error)?,
        [
            size_of::<ValuePublicationPlan<'_>>(),
            product(originals.1.len(), size_of::<Option<ValueBindingData>>())
                .map_err(storage_error)?,
            size_of::<Vec<Option<ValueBindingData>>>(),
            product(
                ordering.len(),
                size_of::<Option<(OrderingMetadata, StateMode)>>(),
            )
            .map_err(storage_error)?,
            size_of::<Vec<Option<(OrderingMetadata, StateMode)>>>(),
        ],
    )
    .map_err(storage_error)?;
    let mut steps = charge.reserve(effects.len(), live).map_err(storage_error)?;
    let live = total(
        live,
        [product(effects.len(), size_of::<ValueStep<'_>>()).map_err(storage_error)?],
    )
    .map_err(storage_error)?;
    charge
        .work(product(effects.len(), 128).map_err(storage_error)?)
        .map_err(storage_error)?;
    for effect in effects {
        let next = build_step(
            changes,
            (effect, inputs),
            originals,
            policies,
            &steps,
            ordering,
            (limits, &mut charge, live),
        )?;
        steps.push(next);
    }
    Ok(ValuePublicationPlan {
        steps,
        prepared: inputs.prepared,
        mode: inputs.mode,
        charge,
    })
}

fn build_step<'plan>(
    changes: &'plan GroupApplyChanges<'_, '_>,
    input: (&RecordBatchValueEffect, &ValueInputs),
    originals: (&[(PartitionId, RecordId)], &[Option<ValueBindingData>]),
    policies: &mut (&[Option<StoredPolicyValue>], &mut PolicySchemaCharge),
    steps: &[ValueStep<'plan>],
    ordering: &[Option<(OrderingMetadata, StateMode)>],
    resources: (&WorkflowValueTransactionLimits, &mut ValueCharge, usize),
) -> Result<ValueStep<'plan>, Error> {
    let (limits, charge, live) = resources;
    let (effect, inputs) = input;
    charge.helper().map_err(storage_error)?;
    charge.candidate().map_err(storage_error)?;
    let source = effect.item_index() as usize;
    let scoped = scoped(changes, source)?;
    let write = scoped.plan().write().ok_or_else(corrupt)?;
    let history = scoped.plan().history().ok_or_else(corrupt)?;
    if write.record.version() != effect.version() || history.version != effect.version() {
        return Err(corrupt());
    }
    let index = query(originals.0, scoped, charge)?;
    let previous = previous(steps, index, originals.1, charge)?;
    let input = *inputs.inputs.get(source).ok_or(Error::InvalidInput)?;
    let order = order(changes, ordering, source, charge)?;
    let binding = if history.kind == EventKind::Created {
        if previous.is_some() {
            return Err(Error::UnsupportedProfile);
        }
        step::birth(
            scoped,
            order,
            input,
            inputs,
            policies.0,
            policies.1,
            (&mut *charge, limits, live),
        )?
    } else {
        previous.ok_or_else(corrupt)?
    };
    let binding = step::advance(
        scoped,
        input,
        binding,
        effect.supplied(),
        changes.plan().control().execution_time(),
        charge,
    )?;
    let final_write = final_write(changes, source, charge)?;
    Ok(ValueStep {
        source_index: effect.item_index(),
        query_index: index,
        plan: scoped,
        ordering: order,
        binding,
        supplied: effect.supplied(),
        final_write,
    })
}

pub(super) fn empty(
    prepared: PreparedDigest,
    mode: u8,
    charge: ValueCharge,
) -> ValuePublicationPlan<'static> {
    ValuePublicationPlan {
        steps: Vec::new(),
        prepared,
        mode,
        charge,
    }
}

fn scoped<'a>(
    changes: &'a GroupApplyChanges<'_, '_>,
    source: usize,
) -> Result<&'a ScopedPlan, Error> {
    if let Some(business) = changes.independent_business() {
        return business
            .results()
            .get(source)
            .and_then(|result| result.as_ref().ok())
            .ok_or_else(corrupt);
    }
    changes
        .business()
        .and_then(|business| business.batch().items().get(source))
        .ok_or_else(corrupt)
}

fn query(
    queries: &[(PartitionId, RecordId)],
    scoped: &ScopedPlan,
    charge: &mut ValueCharge,
) -> Result<usize, Error> {
    let id = scoped.plan().write().ok_or_else(corrupt)?.record.id();
    charge
        .work(product(queries.len(), 128).map_err(storage_error)?)
        .map_err(storage_error)?;
    for (index, (partition, candidate)) in queries.iter().enumerate() {
        charge.candidate().map_err(storage_error)?;
        charge
            .compare(
                partition.as_bytes().len(),
                scoped.partition().as_bytes().len(),
            )
            .map_err(storage_error)?;
        charge
            .compare(candidate.as_bytes().len(), id.as_bytes().len())
            .map_err(storage_error)?;
        if partition == scoped.partition() && candidate == id {
            return Ok(index);
        }
    }
    Err(corrupt())
}

fn previous(
    steps: &[ValueStep<'_>],
    query: usize,
    original: &[Option<ValueBindingData>],
    charge: &mut ValueCharge,
) -> Result<Option<ValueBindingData>, Error> {
    charge
        .work(product(steps.len(), 128).map_err(storage_error)?)
        .map_err(storage_error)?;
    for prior in steps.iter().rev() {
        charge.candidate().map_err(storage_error)?;
        if prior.query_index == query {
            return Ok(Some(prior.binding));
        }
    }
    original.get(query).copied().ok_or_else(corrupt)
}

fn order(
    changes: &GroupApplyChanges<'_, '_>,
    ordering: &[Option<(OrderingMetadata, StateMode)>],
    source: usize,
    charge: &mut ValueCharge,
) -> Result<OrderingMetadata, Error> {
    charge.candidate().map_err(storage_error)?;
    if changes.independent_business().is_some() {
        if !ordering.is_empty() {
            return ordering
                .get(source)
                .copied()
                .flatten()
                .map(|pair| pair.0)
                .ok_or_else(corrupt);
        }
        let business = changes.independent_business().ok_or_else(corrupt)?;
        let mut indices = business.final_indices();
        let mut metadata = business.metadata();
        charge
            .work(product(indices.len(), 128).map_err(storage_error)?)
            .map_err(storage_error)?;
        for _ in 0..indices.len() {
            charge.candidate().map_err(storage_error)?;
            let index = indices.next().ok_or_else(corrupt)?;
            let ordering = metadata.next().ok_or_else(corrupt)?;
            if index == source {
                return Ok(ordering);
            }
        }
        return Err(corrupt());
    }
    let business = changes.business().ok_or_else(corrupt)?;
    let mut metadata = business.metadata();
    charge
        .work(product(metadata.len(), 128).map_err(storage_error)?)
        .map_err(storage_error)?;
    for index in 0..metadata.len() {
        charge.candidate().map_err(storage_error)?;
        let result = metadata.next().ok_or_else(corrupt)?;
        if index == source {
            return Ok(result);
        }
    }
    Err(corrupt())
}

fn final_write(
    changes: &GroupApplyChanges<'_, '_>,
    source: usize,
    charge: &mut ValueCharge,
) -> Result<bool, Error> {
    if let Some(business) = changes.independent_business() {
        charge
            .work(product(business.final_indices().len(), 128).map_err(storage_error)?)
            .map_err(storage_error)?;
        for index in business.final_indices() {
            charge.candidate().map_err(storage_error)?;
            if index == source {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    Ok(true)
}

fn corrupt() -> Error {
    storage_error(StateTransactionError::Corrupt)
}
