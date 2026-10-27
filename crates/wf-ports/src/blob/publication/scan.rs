//! Continuous-work census, one descriptor reserve and immutable verification.

use std::mem::size_of;

use crate::blob::publication::step::{profile, value_effect};
use crate::blob::publication::{
    RecordBatchValueEffect, RecordBatchValueEffects, RecordBatchValueEffectsError as E,
};
use crate::consensus::PreparedOperation;
use crate::log::DecodedRequest;
use wf_domain::workflow::control::ControlPlan;
use wf_domain::workflow::indexed::{IndependentIndexedDecision, IndexedDecision};

const EFFECTS_BASE_WORK: usize = 64;
const EFFECTS_ITEM_WORK: usize = 128;
const EFFECTS_ENCODED_WORK: usize = 4;

type ValueEffectsBuildResult = Result<Option<RecordBatchValueEffects>, (E, Option<(usize, usize)>)>;

pub(super) struct ValueEffectsCharge {
    pub(super) maximum_work: usize,
    pub(super) work: usize,
}

impl ValueEffectsCharge {
    pub(super) fn spend(&mut self, units: usize) -> Result<(), E> {
        let next = self.work.checked_add(units).ok_or(E::Limit)?;
        if next > self.maximum_work {
            return Err(E::Limit);
        }
        self.work = next;
        Ok(())
    }
}

pub(crate) fn value_effects_bounds(
    encoded_length: usize,
    item_count: usize,
) -> Result<(usize, usize), E> {
    let bytes = item_count
        .checked_mul(size_of::<RecordBatchValueEffect>())
        .ok_or(E::Limit)?;
    let work = item_count
        .checked_mul(EFFECTS_ITEM_WORK)
        .and_then(|items| {
            encoded_length
                .checked_mul(EFFECTS_ENCODED_WORK)?
                .checked_add(items)
        })
        .and_then(|work| work.checked_add(EFFECTS_BASE_WORK))
        .ok_or(E::Limit)?;
    Ok((bytes, work))
}

pub(crate) fn build_atomic_value_effects(
    decoded: &DecodedRequest<'_>,
    control: ControlPlan,
    decision: &IndexedDecision<'_>,
    workspace: (usize, usize),
    maximum_work: usize,
) -> ValueEffectsBuildResult {
    let mut charge = ValueEffectsCharge {
        maximum_work,
        work: 0,
    };
    charge.spend(1).map_err(|error| (error, None))?;
    validate_effect_entry(decoded, control, &mut charge).map_err(|error| (error, None))?;
    let count =
        walk_atomic_effects(decoded, decision, &mut charge, None).map_err(|error| (error, None))?;
    charge.spend(1).map_err(|error| (error, None))?;
    let rows = count
        .checked_mul(size_of::<RecordBatchValueEffect>())
        .ok_or((E::Limit, None))?;
    charge.spend(1).map_err(|error| (error, None))?;
    let base = workspace
        .0
        .checked_add(size_of::<Option<RecordBatchValueEffects>>())
        .ok_or((E::Limit, None))?;
    charge.spend(1).map_err(|error| (error, None))?;
    let attempted = base.checked_add(rows).ok_or((E::Limit, None))?;
    charge.spend(1).map_err(|error| (error, None))?;
    if attempted > workspace.1 {
        return Err((E::Limit, Some((attempted, workspace.1))));
    }
    charge.spend(1).map_err(|error| (error, None))?;
    if count == 0 {
        return Ok(None);
    }
    charge.spend(1).map_err(|error| (error, None))?;
    let remaining = workspace.1.checked_sub(base).ok_or((E::Limit, None))?;
    charge.spend(3).map_err(|error| (error, None))?;
    let mut steps = reserve_effects(count, remaining).map_err(|error| (error, None))?;
    let emitted = walk_atomic_effects(decoded, decision, &mut charge, Some(&mut steps))
        .map_err(|error| (error, None))?;
    charge.spend(1).map_err(|error| (error, None))?;
    if emitted != count {
        return Err((E::InvalidChanges, None));
    }
    Ok(Some(RecordBatchValueEffects { steps }))
}

pub(crate) fn build_independent_value_effects(
    decoded: &DecodedRequest<'_>,
    control: ControlPlan,
    decision: &IndependentIndexedDecision<'_>,
    workspace: (usize, usize),
    maximum_work: usize,
) -> ValueEffectsBuildResult {
    let mut charge = ValueEffectsCharge {
        maximum_work,
        work: 0,
    };
    charge.spend(1).map_err(|error| (error, None))?;
    validate_effect_entry(decoded, control, &mut charge).map_err(|error| (error, None))?;
    let count = walk_independent_effects(decoded, decision, &mut charge, None)
        .map_err(|error| (error, None))?;
    charge.spend(1).map_err(|error| (error, None))?;
    let rows = count
        .checked_mul(size_of::<RecordBatchValueEffect>())
        .ok_or((E::Limit, None))?;
    charge.spend(1).map_err(|error| (error, None))?;
    let base = workspace
        .0
        .checked_add(size_of::<Option<RecordBatchValueEffects>>())
        .ok_or((E::Limit, None))?;
    charge.spend(1).map_err(|error| (error, None))?;
    let attempted = base.checked_add(rows).ok_or((E::Limit, None))?;
    charge.spend(1).map_err(|error| (error, None))?;
    if attempted > workspace.1 {
        return Err((E::Limit, Some((attempted, workspace.1))));
    }
    charge.spend(1).map_err(|error| (error, None))?;
    if count == 0 {
        return Ok(None);
    }
    charge.spend(1).map_err(|error| (error, None))?;
    let remaining = workspace.1.checked_sub(base).ok_or((E::Limit, None))?;
    charge.spend(3).map_err(|error| (error, None))?;
    let mut steps = reserve_effects(count, remaining).map_err(|error| (error, None))?;
    let emitted = walk_independent_effects(decoded, decision, &mut charge, Some(&mut steps))
        .map_err(|error| (error, None))?;
    charge.spend(1).map_err(|error| (error, None))?;
    if emitted != count {
        return Err((E::InvalidChanges, None));
    }
    Ok(Some(RecordBatchValueEffects { steps }))
}

pub(super) fn walk_atomic_effects(
    decoded: &DecodedRequest<'_>,
    decision: &IndexedDecision<'_>,
    charge: &mut ValueEffectsCharge,
    mut output: Option<&mut Vec<RecordBatchValueEffect>>,
) -> Result<usize, E> {
    charge.spend(1)?;
    let IndexedDecision::Accepted(batch) = decision else {
        return Ok(0);
    };
    charge.spend(1)?;
    let items = decoded
        .request()
        .operation
        .mutation_items()
        .ok_or(E::UnsupportedProfile)?;
    charge.spend(1)?;
    if batch.batch().items().len() != items.len() {
        return Err(E::InvalidChanges);
    }
    let mut count = 0_usize;
    for (index, plan) in batch.batch().items().iter().enumerate() {
        charge.spend(1)?;
        let effect = value_effect(decoded, index, plan, charge)?;
        charge.spend(1)?;
        if let Some(row) = effect {
            charge.spend(2)?;
            count = count.checked_add(1).ok_or(E::Limit)?;
            append_effect(&mut output, row, charge)?;
        }
        charge.spend(1)?;
    }
    Ok(count)
}

pub(super) fn walk_independent_effects(
    decoded: &DecodedRequest<'_>,
    decision: &IndependentIndexedDecision<'_>,
    charge: &mut ValueEffectsCharge,
    mut output: Option<&mut Vec<RecordBatchValueEffect>>,
) -> Result<usize, E> {
    charge.spend(1)?;
    let IndependentIndexedDecision::Accepted(batch) = decision else {
        return Ok(0);
    };
    charge.spend(1)?;
    let items = decoded
        .request()
        .operation
        .mutation_items()
        .ok_or(E::UnsupportedProfile)?;
    charge.spend(1)?;
    if batch.results().len() != items.len() {
        return Err(E::InvalidChanges);
    }
    let mut count = 0_usize;
    for (index, result) in batch.results().iter().enumerate() {
        charge.spend(2)?;
        if let Ok(plan) = result {
            let effect = value_effect(decoded, index, plan, charge)?;
            charge.spend(1)?;
            if let Some(row) = effect {
                charge.spend(2)?;
                count = count.checked_add(1).ok_or(E::Limit)?;
                append_effect(&mut output, row, charge)?;
            }
        }
        charge.spend(1)?;
    }
    Ok(count)
}

pub(super) fn check_atomic_effects(
    decoded: &DecodedRequest<'_>,
    decision: &IndexedDecision<'_>,
    rows: &[RecordBatchValueEffect],
    charge: &mut ValueEffectsCharge,
) -> Result<(), E> {
    charge.spend(1)?;
    let IndexedDecision::Accepted(batch) = decision else {
        return if rows.is_empty() {
            Ok(())
        } else {
            Err(E::InvalidChanges)
        };
    };
    charge.spend(1)?;
    let items = decoded
        .request()
        .operation
        .mutation_items()
        .ok_or(E::UnsupportedProfile)?;
    charge.spend(1)?;
    if batch.batch().items().len() != items.len() {
        return Err(E::InvalidChanges);
    }
    let mut count = 0_usize;
    for (index, plan) in batch.batch().items().iter().enumerate() {
        charge.spend(1)?;
        let effect = value_effect(decoded, index, plan, charge)?;
        charge.spend(1)?;
        if let Some(expected) = effect {
            charge.spend(1)?;
            let row = rows.get(count).ok_or(E::InvalidChanges)?;
            charge.spend(1)?;
            if row != &expected {
                return Err(E::InvalidChanges);
            }
            charge.spend(1)?;
            count = count.checked_add(1).ok_or(E::Limit)?;
        }
        charge.spend(1)?;
    }
    charge.spend(1)?;
    if count != rows.len() {
        return Err(E::InvalidChanges);
    }
    Ok(())
}

pub(super) fn check_independent_effects(
    decoded: &DecodedRequest<'_>,
    decision: &IndependentIndexedDecision<'_>,
    rows: &[RecordBatchValueEffect],
    charge: &mut ValueEffectsCharge,
) -> Result<(), E> {
    charge.spend(1)?;
    let IndependentIndexedDecision::Accepted(batch) = decision else {
        return if rows.is_empty() {
            Ok(())
        } else {
            Err(E::InvalidChanges)
        };
    };
    charge.spend(1)?;
    let items = decoded
        .request()
        .operation
        .mutation_items()
        .ok_or(E::UnsupportedProfile)?;
    charge.spend(1)?;
    if batch.results().len() != items.len() {
        return Err(E::InvalidChanges);
    }
    let mut count = 0_usize;
    for (index, result) in batch.results().iter().enumerate() {
        charge.spend(2)?;
        if let Ok(plan) = result {
            let effect = value_effect(decoded, index, plan, charge)?;
            charge.spend(1)?;
            if let Some(expected) = effect {
                verify_effect(rows, &mut count, &expected, charge)?;
            }
        }
        charge.spend(1)?;
    }
    charge.spend(1)?;
    if count != rows.len() {
        return Err(E::InvalidChanges);
    }
    Ok(())
}

pub(super) fn validate_effect_entry(
    decoded: &DecodedRequest<'_>,
    control: ControlPlan,
    charge: &mut ValueEffectsCharge,
) -> Result<(), E> {
    charge.spend(1)?;
    let entry = control.next().last_entry().ok_or(E::InvalidChanges)?;
    charge.spend(64)?;
    if entry.prepared() != decoded.digest() {
        return Err(E::InvalidChanges);
    }
    charge.spend(1)?;
    let version = profile(decoded, charge)?;
    if !matches!(version, 1 | 5..=7) {
        return Err(E::UnsupportedProfile);
    }
    charge.spend(1)?;
    let items = decoded
        .request()
        .operation
        .mutation_items()
        .ok_or(E::UnsupportedProfile)?;
    if items.is_empty() || (matches!(version, 1 | 7) && items.len() != 1) {
        return Err(E::UnsupportedProfile);
    }
    charge.spend(1)?;
    if (version == 1)
        != matches!(
            decoded.request().operation,
            PreparedOperation::PublicMutation { .. }
        )
    {
        return Err(E::UnsupportedProfile);
    }
    Ok(())
}

pub(super) fn reserve_effects(
    count: usize,
    maximum_bytes: usize,
) -> Result<Vec<RecordBatchValueEffect>, E> {
    let bytes = count
        .checked_mul(size_of::<RecordBatchValueEffect>())
        .ok_or(E::Limit)?;
    if bytes > maximum_bytes {
        return Err(E::Limit);
    }
    let mut rows = Vec::new();
    rows.try_reserve_exact(count).map_err(E::ReserveFailed)?;
    Ok(rows)
}

fn append_effect(
    output: &mut Option<&mut Vec<RecordBatchValueEffect>>,
    row: RecordBatchValueEffect,
    charge: &mut ValueEffectsCharge,
) -> Result<(), E> {
    if let Some(output) = output.as_deref_mut() {
        charge.spend(1)?;
        if output.len() >= output.capacity() {
            return Err(E::Limit);
        }
        charge.spend(1)?;
        output.push(row);
    }
    Ok(())
}

fn verify_effect(
    rows: &[RecordBatchValueEffect],
    count: &mut usize,
    expected: &RecordBatchValueEffect,
    charge: &mut ValueEffectsCharge,
) -> Result<(), E> {
    charge.spend(1)?;
    let row = rows.get(*count).ok_or(E::InvalidChanges)?;
    charge.spend(1)?;
    if row != expected {
        return Err(E::InvalidChanges);
    }
    charge.spend(1)?;
    *count = count.checked_add(1).ok_or(E::Limit)?;
    Ok(())
}
