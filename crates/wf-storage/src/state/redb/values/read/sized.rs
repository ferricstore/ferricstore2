//! One authenticated snapshot; omission avoids only the owned stored-byte copy.

use std::mem::size_of;
use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::{
    WorkflowValueReadFailure, WorkflowValueReadLimits, WorkflowValueSizedReadBatch,
    WorkflowValueSizedReadValue,
};
use wf_ports::clock::WorkflowValueReadClock;
use wf_ports::state::StateTransactionError as Error;
use wf_types::version::{HlcTimestamp, TimestampMillis};

use super::super::budget::{ValueCharge, product, total};
use super::RedbStateStore;
use crate::state::value;

mod captured;
mod point;
pub(in crate::state::redb) use captured::read_captured;
use point::one;

pub(in super::super) fn read_sized<C: WorkflowValueReadClock>(
    store: &RedbStateStore,
    clock: &mut C,
    scope: &RequestScope,
    references: &[&[u8]],
    omission_threshold: usize,
    limits: WorkflowValueReadLimits,
) -> Result<WorkflowValueSizedReadBatch, Error> {
    store.ready()?;
    super::check_scope(store, scope)?;
    if references.len() > limits.maximum_references {
        return Err(Error::Limit);
    }
    if omission_threshold == 0 {
        return Err(Error::InvalidChanges);
    }
    let mut charge = ValueCharge::new(
        limits.maximum_references.max(1),
        limits.maximum_stored_bytes,
        limits.maximum_work,
        limits.maximum_temporary_bytes,
    )?;
    let fixed = metadata(scope)?;
    let slot_bytes = product(
        references.len(),
        size_of::<Result<Option<usize>, WorkflowValueReadFailure>>(),
    )?;
    let value_bytes = product(references.len(), size_of::<WorkflowValueSizedReadValue>())?;
    let eligible_count = count_eligible(references, &mut charge)?;
    let mut slots = charge.reserve(references.len(), fixed)?;
    let mut values = charge.reserve(references.len(), total(fixed, [slot_bytes])?)?;
    let live = total(fixed, [slot_bytes, value_bytes])?;
    if eligible_count == 0 {
        charge.work(product(references.len(), 128)?)?;
        slots.resize(references.len(), Ok(None));
        return Ok(WorkflowValueSizedReadBatch { values, slots });
    }
    if !matches!(store.layout.get(), 7 | 8) {
        return Err(Error::UnsupportedLayout);
    }
    let mut context = super::open(store, charge).map_err(|e| store.failed(e))?;
    context.charge.candidate()?;
    // Clock failure propagates without the engine-failure fence.
    let time = clock.read_snapshot()?;
    let mut returned = 0;
    context.charge.work(product(references.len(), 128)?)?;
    for (index, reference) in references.iter().enumerate() {
        context
            .charge
            .work(total(128, [product(reference.len(), 4)?])?)?;
        if !super::eligible(reference) {
            context.charge.candidate()?;
            slots.push(Ok(None));
            continue;
        }
        if let Some(previous) = duplicate(reference, &references[..index], &mut context.charge)? {
            context.charge.candidate()?;
            slots.push(slots[previous]);
            continue;
        }
        let next = one(
            &mut context,
            scope,
            reference,
            time,
            omission_threshold,
            total(live, [returned])?,
        )
        .map_err(|e| store.failed(e))?;
        context.charge.candidate()?;
        let slot = match next {
            Ok(Some(value)) => {
                if let WorkflowValueSizedReadValue::Stored(bytes) = &value {
                    returned = total(returned, [bytes.len()])?;
                }
                let position = values.len();
                context.charge.candidate()?;
                values.push(value);
                Ok(Some(position))
            }
            Ok(None) => Ok(None),
            Err(error) => Err(error),
        };
        context.charge.candidate()?;
        slots.push(slot);
    }
    super::close(context);
    Ok(WorkflowValueSizedReadBatch { values, slots })
}

fn metadata(scope: &RequestScope) -> Result<usize, Error> {
    // Conservative control cells: threshold, eligible_count, fixed, slot_bytes,
    // value_bytes, live, returned, index, previous, position, bound, size,
    // immutable_bound, alias_key_bytes, alias_bytes and guard_live.
    total(
        super::fixed_owner()?,
        [
            size_of::<WorkflowValueSizedReadBatch>(),
            size_of::<WorkflowValueReadLimits>(),
            size_of::<Option<WorkflowValueSizedReadValue>>(),
            size_of::<Result<Option<WorkflowValueSizedReadValue>, WorkflowValueReadFailure>>(),
            size_of::<Result<Option<usize>, WorkflowValueReadFailure>>(),
            size_of::<Option<usize>>(),
            size_of::<value::ValueAlias<'_>>(),
            size_of::<(HlcTimestamp, TimestampMillis)>(),
            size_of::<[usize; 16]>(),
            size_of::<[bool; 4]>(),
            product(3, size_of::<Vec<u8>>())?,
            product(2, size_of::<redb::AccessGuard<'_, &[u8]>>())?,
            product(2, size_of::<Option<redb::AccessGuard<'_, &[u8]>>>())?,
            size_of::<sha2::Sha256>(),
            size_of::<[u8; 32]>(),
            total(
                208,
                [
                    scope.group().as_bytes().len(),
                    scope.recovery().as_bytes().len(),
                ],
            )?,
        ],
    )
}

fn count_eligible(references: &[&[u8]], charge: &mut ValueCharge) -> Result<usize, Error> {
    charge.work(product(references.len(), 128)?)?;
    let mut eligible_count = 0;
    for reference in references {
        charge.work(total(128, [product(reference.len(), 4)?])?)?;
        eligible_count = total(eligible_count, [usize::from(super::eligible(reference))])?;
    }
    Ok(eligible_count)
}

fn duplicate(
    reference: &[u8],
    previous: &[&[u8]],
    charge: &mut ValueCharge,
) -> Result<Option<usize>, Error> {
    charge.candidate()?;
    for (index, candidate) in previous.iter().enumerate() {
        charge.candidate()?;
        charge.compare(reference.len(), candidate.len())?;
        if reference == *candidate {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests;
