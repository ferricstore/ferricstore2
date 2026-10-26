//! Source cardinality restoration and complete ordered per-occurrence writes.

use std::mem::size_of;

use super::budget::{HistoryValueCharge, add, mul, reserve};
use super::decode;
use super::model::{
    HistoryValueError as E, HistoryValueKind as K, HistoryValueLimits as L, HistoryValuePlan,
    HistoryValueReadBatch as B, HistoryValueReadResult as R, HistoryValueReference as Reference,
    HistoryValueReturn,
};
use super::overlay::{
    HistoryValue as V, HistoryValueField as F, HistoryValueOverlay, HistoryValueWrite,
};
use crate::value::OriginalValueContext;

pub(super) fn apply<'a, T: ?Sized>(
    mut plan: HistoryValuePlan<'a, T>,
    batch: Option<B<'_>>,
    context: &OriginalValueContext<'_>,
) -> Result<HistoryValueOverlay<'a, T>, E> {
    plan.charge.events(&plan.limits, 2)?;
    if plan.references.is_empty() != batch.is_none() {
        return Err(E::InvalidReadBatch);
    }
    let slots = if let Some(batch) = &batch {
        indices(batch, plan.references.len(), &plan.limits, &mut plan.charge)?
    } else {
        Vec::new()
    };
    let values = match &batch {
        Some(B::Values(values) | B::Filtered { values, .. }) => Some(values.as_slice()),
        _ => None,
    };
    let mut writes = Vec::new();
    if matches!(plan.value_return, HistoryValueReturn::Enabled { .. }) {
        let history = plan.history;
        plan.charge.events(&plan.limits, mul(history.len(), 4)?)?;
        for (index, entry) in history.iter().enumerate() {
            for (kind, reference) in [
                (K::Payload, entry.references[0]),
                (K::Result, entry.references[1]),
                (K::Error, entry.references[2]),
            ] {
                let selected = match reference {
                    Reference::Binary(bytes) if !bytes.is_empty() && bytes.len() <= 65_535 => {
                        lookup(&mut plan, bytes, &slots, values)?
                    }
                    _ => None,
                };
                occurrence(
                    &mut plan,
                    &mut writes,
                    index,
                    kind,
                    reference,
                    selected,
                    context,
                )?;
            }
        }
    }
    let overlay = HistoryValueOverlay {
        history: plan.history,
        writes,
        charged_work: plan.charge.work_units,
        charged_requested_bytes: plan.charge.requested_bytes,
    };
    drop(batch);
    Ok(overlay)
}

fn indices(
    batch: &B<'_>,
    count: usize,
    limits: &L,
    charge: &mut HistoryValueCharge,
) -> Result<Vec<Option<usize>>, E> {
    charge.events(limits, 2)?;
    let values = match batch {
        B::Values(values) | B::Filtered { values, .. } => values,
        B::NonList => return Err(E::InvalidReadBatch),
    };
    if let B::Filtered { eligibility, .. } = batch
        && eligibility.len() != count
    {
        return Err(E::InvalidReadBatch);
    }
    if values.len() > limits.maximum_read_results {
        return Err(E::Limit);
    }
    charge.pay(limits, 0, mul(values.capacity(), size_of::<R<'_>>())?)?;
    if let B::Filtered { eligibility, .. } = batch {
        // Fund both incoming spines before allocating normalized slots.
        charge.pay(limits, 0, mul(eligibility.capacity(), size_of::<bool>())?)?;
    }
    charge.events(limits, values.len())?;
    for value in values {
        if let R::Stored(bytes) = value {
            charge.pay(limits, 0, bytes.capacity())?;
        }
    }
    let mut slots = Vec::new();
    reserve(&mut slots, count, limits, charge)?;
    match batch {
        B::Values(_) => zip_slots(&mut slots, count, values.len(), limits, charge)?,
        B::Filtered { eligibility, .. } => {
            charge.events(limits, eligibility.len())?;
            let mut eligible = 0;
            for flag in eligibility {
                eligible = add(eligible, usize::from(*flag))?;
            }
            if eligible == count {
                zip_slots(&mut slots, count, values.len(), limits, charge)?;
            } else if eligible != values.len() {
                charge.events(limits, count)?;
                slots.resize(count, None);
            } else {
                filtered_slots(&mut slots, eligibility, limits, charge)?;
            }
        }
        B::NonList => return Err(E::InvalidReadBatch),
    }
    Ok(slots)
}

fn zip_slots(
    slots: &mut Vec<Option<usize>>,
    count: usize,
    available: usize,
    limits: &L,
    charge: &mut HistoryValueCharge,
) -> Result<(), E> {
    charge.events(limits, count)?;
    for index in 0..count {
        slots.push((index < available).then_some(index));
    }
    Ok(())
}

fn filtered_slots(
    slots: &mut Vec<Option<usize>>,
    mask: &[bool],
    limits: &L,
    charge: &mut HistoryValueCharge,
) -> Result<(), E> {
    charge.events(limits, mask.len())?;
    let mut index = 0;
    for flag in mask {
        if *flag {
            slots.push(Some(index));
            index = add(index, 1)?;
        } else {
            slots.push(None);
        }
    }
    Ok(())
}

fn lookup<'read, 'value, T: ?Sized>(
    plan: &mut HistoryValuePlan<'_, T>,
    reference: &[u8],
    slots: &[Option<usize>],
    values: Option<&'read [R<'value>]>,
) -> Result<Option<&'read R<'value>>, E> {
    plan.charge
        .events(&plan.limits, add(1, plan.references.len())?)?;
    for (index, candidate) in plan.references.iter().enumerate() {
        plan.charge
            .compare(&plan.limits, reference.len(), candidate.len())?;
        if reference == *candidate {
            return Ok(slots
                .get(index)
                .copied()
                .flatten()
                .and_then(|slot| values?.get(slot)));
        }
    }
    Ok(None)
}

fn occurrence<T: ?Sized>(
    plan: &mut HistoryValuePlan<'_, T>,
    writes: &mut Vec<HistoryValueWrite>,
    index: usize,
    kind: K,
    reference: Reference<'_>,
    result: Option<&R<'_>>,
    context: &OriginalValueContext<'_>,
) -> Result<(), E> {
    plan.charge.events(&plan.limits, 1)?;
    let Reference::Binary(reference) = reference else {
        return Ok(());
    };
    if reference.is_empty() {
        return Ok(());
    }
    if reference.len() > 65_535 {
        let message = match kind {
            K::Payload => "ERR payload_ref key too large",
            K::Result => "ERR result_ref key too large",
            K::Error => "ERR error_ref key too large",
        };
        return write(plan, writes, index, F::Error(kind), V::Error(message));
    }
    match result {
        None | Some(R::Missing) => {
            write(plan, writes, index, F::Value(kind), V::Nil)?;
            write(plan, writes, index, F::Missing(kind), V::Boolean(true))
        }
        Some(R::StorageFailure { .. }) => write(
            plan,
            writes,
            index,
            F::Error(kind),
            V::Error("ERR storage read failed"),
        ),
        Some(R::StoredSize { bytes }) => omitted(plan, writes, index, kind, *bytes),
        Some(R::Stored(bytes)) => stored(plan, writes, index, kind, bytes, context),
        Some(R::Unrelated) => Ok(()),
    }
}

fn stored<T: ?Sized>(
    plan: &mut HistoryValuePlan<'_, T>,
    writes: &mut Vec<HistoryValueWrite>,
    index: usize,
    kind: K,
    bytes: &[u8],
    context: &OriginalValueContext<'_>,
) -> Result<(), E> {
    plan.charge.events(&plan.limits, 1)?;
    let HistoryValueReturn::Enabled { max_bytes } = plan.value_return else {
        return Ok(());
    };
    if i128::try_from(bytes.len()).map_err(|_| E::Limit)? > max_bytes {
        return omitted(plan, writes, index, kind, bytes.len());
    }
    match decode::value(bytes, context, &plan.limits, &mut plan.charge)? {
        Some(document) => {
            write(plan, writes, index, F::Value(kind), V::Decoded(document))?;
            write(
                plan,
                writes,
                index,
                F::Size(kind),
                V::StoredSize(bytes.len()),
            )
        }
        None => write(
            plan,
            writes,
            index,
            F::Error(kind),
            V::Error("ERR invalid flow value"),
        ),
    }
}

fn omitted<T: ?Sized>(
    plan: &mut HistoryValuePlan<'_, T>,
    writes: &mut Vec<HistoryValueWrite>,
    index: usize,
    kind: K,
    bytes: usize,
) -> Result<(), E> {
    plan.charge.events(&plan.limits, 1)?;
    write(plan, writes, index, F::Omitted(kind), V::Boolean(true))?;
    write(plan, writes, index, F::Size(kind), V::StoredSize(bytes))
}

fn write<T: ?Sized>(
    plan: &mut HistoryValuePlan<'_, T>,
    writes: &mut Vec<HistoryValueWrite>,
    index: usize,
    field: F,
    value: V,
) -> Result<(), E> {
    plan.charge.events(&plan.limits, 1)?;
    let target = add(writes.len(), 1)?;
    if target > plan.limits.maximum_writes {
        return Err(E::Limit);
    }
    reserve(writes, target, &plan.limits, &mut plan.charge)?;
    writes.push(HistoryValueWrite {
        entry_index: index,
        field,
        value,
    });
    Ok(())
}
