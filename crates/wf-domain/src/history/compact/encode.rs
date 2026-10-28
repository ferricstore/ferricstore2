//! One shared measure/emit grammar with no wrapped intermediate buffer.

use std::mem::size_of;

use crate::{
    history::metadata::OriginalHistoryMetadataField, metadata::OriginalMetadataInput as M,
};

use super::{
    OriginalCompactHistoryError as E, OriginalCompactHistoryLimits as L,
    OriginalCompactHistorySnapshot as S, PreparedOriginalCompactHistory as P,
    budget::{Budget, product},
    number,
    primitives::Output,
};

pub(super) fn run(snapshot: &S<'_, '_>, limits: L) -> Result<P, E> {
    let mut budget = Budget::new(limits)?;
    input(snapshot, &mut budget)?;
    let flags = flags(snapshot, &mut budget)?;
    let mut measured = Output::new(None, limits.max_encoded_bytes);
    emit(snapshot, flags, &mut measured, &mut budget)?;
    let length = measured.len();
    let mut bytes = budget.reserve(length)?;
    let mut output = Output::new(Some(&mut bytes), length);
    emit(snapshot, flags, &mut output, &mut budget)?;
    if output.len() != length {
        return Err(E::InvalidInput);
    }
    Ok(P {
        bytes,
        work: budget.work,
        temporary: budget.peak,
    })
}

fn input(snapshot: &S<'_, '_>, budget: &mut Budget) -> Result<(), E> {
    budget.input(size_of::<S<'_, '_>>())?;
    budget.input(snapshot.event.len())?;
    if let Some(state) = snapshot.state {
        budget.input(state.len())?;
    }
    number::input(snapshot.version, budget)?;
    number::input(snapshot.at, budget)?;
    for number in snapshot.numbers {
        number::input(number, budget)?;
    }
    for bytes in snapshot.references.into_iter().flatten() {
        budget.input(bytes.len())?;
    }
    budget.context(snapshot.context)?;
    budget.pairs(snapshot.metadata.len())?;
    budget.input(product(
        snapshot.metadata.len(),
        size_of::<OriginalHistoryMetadataField<'_>>(),
    )?)?;
    for field in snapshot.metadata {
        budget.input(field.key.len())?;
        budget.input(field.value.len())?;
    }
    Ok(())
}

fn flags(snapshot: &S<'_, '_>, budget: &mut Budget) -> Result<u16, E> {
    let mut flags = 0;
    for (bit, value) in snapshot.numbers.into_iter().enumerate() {
        budget.spend(1)?;
        if !number::is_integer(value) {
            continue;
        }
        let present = match bit {
            3 | 4 => !number::equal(value, snapshot.at, budget)?,
            5 => true,
            _ => !number::equal(value, M::Integer(0), budget)?,
        };
        if present {
            flags |= 1 << bit;
        }
    }
    for (bit, value) in snapshot.references.into_iter().enumerate() {
        budget.spend(1)?;
        if value.is_some_and(|bytes| !bytes.is_empty()) {
            flags |= 1 << (bit + 7);
        }
    }
    budget.spend(1)?;
    if !snapshot.metadata.is_empty() {
        flags |= 1 << 12;
    }
    Ok(flags)
}

fn emit(
    snapshot: &S<'_, '_>,
    flags: u16,
    output: &mut Output<'_>,
    budget: &mut Budget,
) -> Result<(), E> {
    output.put(b"FSH2", budget)?;
    output.integer(Some(u128::from(flags)), budget)?;
    output.binary(Some(snapshot.event), budget)?;
    number::write(snapshot.version, output, budget)?;
    number::write(snapshot.at, output, budget)?;
    output.binary(snapshot.state, budget)?;
    for (bit, value) in snapshot.numbers.into_iter().enumerate() {
        budget.spend(1)?;
        if flags & (1 << bit) != 0 {
            number::write(value, output, budget)?;
        }
    }
    for (bit, value) in snapshot.references.into_iter().enumerate() {
        budget.spend(1)?;
        if flags & (1 << (bit + 7)) != 0 {
            output.binary(value, budget)?;
        }
    }
    if flags & (1 << 12) != 0 {
        output.integer(Some(snapshot.metadata.len() as u128), budget)?;
        for field in snapshot.metadata {
            output.binary(Some(field.key.as_ref()), budget)?;
            output.binary(Some(field.value.as_ref()), budget)?;
        }
    }
    Ok(())
}
