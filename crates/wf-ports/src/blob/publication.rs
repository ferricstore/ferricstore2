//! Numeric value provenance borrowed from one exact prepared apply plan.

use std::mem::size_of;

use crate::log::{DecodedRequest, RecordBatchApplyDecision, RecordBatchApplyPlan};

mod scan;
mod step;

pub(crate) use scan::{
    build_atomic_value_effects, build_independent_value_effects, value_effects_bounds,
};

use scan::{
    ValueEffectsCharge, check_atomic_effects, check_independent_effects, validate_effect_entry,
    walk_atomic_effects, walk_independent_effects,
};

/// One original input position with a real accepted record/history mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordBatchValueEffect {
    item_index: u32,
    version: u64,
    supplied: [bool; 3],
    terminal: bool,
}

impl RecordBatchValueEffect {
    /// Reads the original source item index, including independent-result holes.
    #[must_use]
    pub const fn item_index(&self) -> u32 {
        self.item_index
    }

    /// Reads the accepted record/history version.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Reads explicit field presence in payload, result, error order.
    #[must_use]
    pub const fn supplied(&self) -> [bool; 3] {
        self.supplied
    }

    /// Reads terminal postimage status; this grants no retention or expiry proof.
    #[must_use]
    pub const fn terminal(&self) -> bool {
        self.terminal
    }
}

/// Numeric rows owned by the same Fresh plan, without publication authority.
#[derive(Debug, Eq, PartialEq)]
pub struct RecordBatchValueEffects {
    steps: Vec<RecordBatchValueEffect>,
}

impl RecordBatchValueEffects {
    /// Borrows every real history row in original source input order.
    #[must_use]
    pub fn steps(&self) -> &[RecordBatchValueEffect] {
        &self.steps
    }
}

/// Source association, supported profile and operational resource failures.
#[derive(Debug)]
pub enum RecordBatchValueEffectsError {
    /// The decoded command and accepted plan do not describe the same changes.
    InvalidChanges,
    /// This profile or a required attached provenance owner is unavailable.
    UnsupportedProfile,
    /// Checked descriptor-byte or continuous-work admission refused.
    Limit,
    /// A real fallible descriptor reservation failed.
    ReserveFailed(std::collections::TryReserveError),
}

/// Inspects numeric provenance without borrowing the verification descriptor.
///
/// The returned owner is attached to the same plan. This grants no storage,
/// expiry, placement or durable apply authority and allocates no output.
///
/// # Errors
/// Rejects crossed changes, unavailable profiles and explicit byte/work limits.
pub fn inspect_record_batch_value_effects<'plan>(
    decoded: &DecodedRequest<'_>,
    plan: &'plan RecordBatchApplyPlan<'_>,
    maximum_effect_bytes: usize,
    maximum_work: usize,
) -> Result<Option<&'plan RecordBatchValueEffects>, RecordBatchValueEffectsError> {
    let mut charge = ValueEffectsCharge {
        maximum_work,
        work: 0,
    };
    charge.spend(1)?;
    validate_effect_entry(decoded, plan.control(), &mut charge)?;
    charge.spend(1)?;
    let attached = plan.value_effects();
    let count = match (plan.decision(), attached) {
        (RecordBatchApplyDecision::Fresh { operation, .. }, Some(effects)) => {
            check_atomic_effects(decoded, operation, effects.steps(), &mut charge)?;
            effects.steps().len()
        }
        (RecordBatchApplyDecision::IndependentFresh { operation, .. }, Some(effects)) => {
            check_independent_effects(decoded, operation, effects.steps(), &mut charge)?;
            effects.steps().len()
        }
        (RecordBatchApplyDecision::Fresh { operation, .. }, None) => {
            walk_atomic_effects(decoded, operation, &mut charge, None)?
        }
        (RecordBatchApplyDecision::IndependentFresh { operation, .. }, None) => {
            walk_independent_effects(decoded, operation, &mut charge, None)?
        }
        (_, None) => 0,
        (_, Some(_)) => return Err(RecordBatchValueEffectsError::InvalidChanges),
    };
    charge.spend(1)?;
    if count != 0 && attached.is_none() {
        return Err(RecordBatchValueEffectsError::UnsupportedProfile);
    }
    charge.spend(1)?;
    let bytes = count
        .checked_mul(size_of::<RecordBatchValueEffect>())
        .ok_or(RecordBatchValueEffectsError::Limit)?;
    charge.spend(1)?;
    if bytes > maximum_effect_bytes {
        return Err(RecordBatchValueEffectsError::Limit);
    }
    charge.spend(1)?;
    match (count, attached) {
        (0, None) => Ok(None),
        (0, Some(_)) => Err(RecordBatchValueEffectsError::InvalidChanges),
        (_, Some(effects)) => Ok(Some(effects)),
        (_, None) => Err(RecordBatchValueEffectsError::UnsupportedProfile),
    }
}
