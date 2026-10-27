//! One authenticated snapshot; omission avoids only the owned stored-byte copy.

use std::mem::size_of;
use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::{
    OriginalValueRef, OriginalValueRefKind, WorkflowValueReadFailure, WorkflowValueReadLimits,
    WorkflowValueSizedReadBatch, WorkflowValueSizedReadValue, parse_original_value_ref,
};
use wf_ports::clock::WorkflowValueReadClock;
use wf_ports::state::StateTransactionError as Error;
use wf_types::version::{HlcTimestamp, TimestampMillis};

use super::super::budget::{ValueCharge, product, total};
use super::{RedbStateStore, ValueReadContext, admission, scope_bytes};
use crate::state::{key, value};

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
    if store.layout.get() != 7 {
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

fn one(
    context: &mut ValueReadContext,
    scope: &RequestScope,
    reference: &[u8],
    time: (HlcTimestamp, TimestampMillis),
    threshold: usize,
    live: usize,
) -> Result<Result<Option<WorkflowValueSizedReadValue>, WorkflowValueReadFailure>, Error> {
    context.charge.helper()?;
    context.charge.work(product(reference.len(), 4)?)?;
    let Some(OriginalValueRef::Regular {
        kind:
            OriginalValueRefKind::Payload | OriginalValueRefKind::Result | OriginalValueRefKind::Error,
        version,
        ..
    }) = parse_original_value_ref(reference)
    else {
        return Ok(Err(WorkflowValueReadFailure::UnsupportedProfile));
    };
    if version == 0 {
        return Ok(Ok(None));
    }
    context.charge.work(total(
        128,
        [product(total(reference.len(), [scope_bytes(scope)])?, 16)?],
    )?)?;
    let bound = total(
        64,
        [product(total(reference.len(), [scope_bytes(scope)])?, 2)?],
    )?;
    context.charge.temporary(total(live, [bound])?)?;
    let alias_key = key::encode_alias_key(
        scope,
        reference,
        bound.min(context.limits.maximum_temporary_bytes()),
    )
    .map_err(admission::value_error)?;
    context.charge.candidate()?;
    let Some(raw) = context
        .rows
        .get(alias_key.as_slice())
        .map_err(|e| admission::storage_error(&e))?
    else {
        return Ok(Ok(None));
    };
    context.charge.work(total(128, [raw.value().len()])?)?;
    context
        .charge
        .temporary(total(live, [alias_key.len(), raw.value().len()])?)?;
    let alias = super::checked_alias(
        raw.value(),
        reference,
        &mut context.charge,
        total(live, [alias_key.len()])?,
        context.alias_format,
    )?;
    immutable(
        &context.rows,
        &mut context.charge,
        scope,
        reference,
        (alias, alias_key.len(), raw.value().len(), bound),
        time,
        (threshold, live),
    )
}

fn immutable(
    rows: &redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    charge: &mut ValueCharge,
    scope: &RequestScope,
    reference: &[u8],
    (alias, alias_key_bytes, alias_bytes, bound): (value::ValueAlias<'_>, usize, usize, usize),
    time: (HlcTimestamp, TimestampMillis),
    (threshold, live): (usize, usize),
) -> Result<Result<Option<WorkflowValueSizedReadValue>, WorkflowValueReadFailure>, Error> {
    let size = usize::try_from(alias.stored_len).map_err(|_| Error::Limit)?;
    charge.work(total(
        128,
        [product(total(reference.len(), [scope_bytes(scope)])?, 16)?],
    )?)?;
    let immutable_bound = total(bound, [16])?;
    let guard_live = total(live, [alias_key_bytes, alias_bytes, immutable_bound, size])?;
    charge.temporary(guard_live)?;
    let immutable = key::encode_immutable_key(scope, reference, alias.incarnation, immutable_bound)
        .map_err(admission::value_error)?;
    // Fund the complete guard extent and hash before acquiring the immutable row.
    charge.work(total(128, [size])?)?;
    charge.candidate()?;
    let bytes = rows
        .get(immutable.as_slice())
        .map_err(|e| admission::storage_error(&e))?
        .ok_or(Error::Corrupt)?;
    // The shared predicate checks actual length before traversing any bytes.
    super::authenticate(bytes.value(), size, &alias.digest)?;
    charge.candidate()?;
    match super::expiry(alias.expires_at, time.0, time.1) {
        Err(error) => return Ok(Err(error)),
        Ok(false) => return Ok(Ok(None)),
        Ok(true) => {}
    }
    charge.candidate()?;
    if size >= threshold {
        return Ok(Ok(Some(WorkflowValueSizedReadValue::Omitted {
            stored_bytes: size,
        })));
    }
    // The current guard extent remains live while the independent copy is funded.
    super::owned_value(charge, bytes.value(), guard_live)
        .map(|bytes| Ok(Some(WorkflowValueSizedReadValue::Stored(bytes))))
}

#[cfg(test)]
mod tests;
