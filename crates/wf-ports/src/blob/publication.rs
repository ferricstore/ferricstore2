//! Numeric value provenance borrowed from one exact prepared apply plan.

use crate::log::{DecodedRequest, RecordBatchApplyPlan};

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
/// This callable rejecting baseline grants no storage, expiry or apply authority.
///
/// # Errors
/// The baseline reports an unsupported provenance profile for every input.
pub fn inspect_record_batch_value_effects<'plan>(
    _decoded: &DecodedRequest<'_>,
    _plan: &'plan RecordBatchApplyPlan<'_>,
    _maximum_effect_bytes: usize,
    _maximum_work: usize,
) -> Result<Option<&'plan RecordBatchValueEffects>, RecordBatchValueEffectsError> {
    Err(RecordBatchValueEffectsError::UnsupportedProfile)
}
