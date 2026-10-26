//! Pure value collection and ordered writes over borrowed original history.
//!
//! Captured references and read results carry no storage or routing authority.
//! Requested-byte counters are conservative cumulative ownership liabilities.

mod budget;
mod model;
mod overlay;

pub use model::{
    HistoryValueEntry, HistoryValueError, HistoryValueKind, HistoryValueLimits, HistoryValuePlan,
    HistoryValueReadBatch, HistoryValueReadResult, HistoryValueReference, HistoryValueReturn,
};
pub use overlay::{HistoryValue, HistoryValueField, HistoryValueOverlay, HistoryValueWrite};

use crate::value::OriginalValueContext;

/// Prepare stable references for a caller's later captured read.
///
/// # Errors
///
/// Returns an operational error when the request cannot be admitted.
pub fn prepare_history_values<'a, T: ?Sized>(
    _history: &'a [HistoryValueEntry<'a, T>],
    _value_return: HistoryValueReturn,
    _limits: HistoryValueLimits,
) -> Result<HistoryValuePlan<'a, T>, HistoryValueError> {
    Err(HistoryValueError::Limit)
}

/// Apply captured results as ordered writes without changing original entries.
///
/// # Errors
///
/// Returns malformed-batch, operational decode, resource or reservation errors.
pub fn apply_history_value_results<'a, T: ?Sized>(
    _plan: HistoryValuePlan<'a, T>,
    _batch: Option<HistoryValueReadBatch<'_>>,
    _context: &OriginalValueContext<'_>,
) -> Result<HistoryValueOverlay<'a, T>, HistoryValueError> {
    Err(HistoryValueError::Limit)
}
