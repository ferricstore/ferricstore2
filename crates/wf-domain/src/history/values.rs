//! Pure value collection and ordered writes over borrowed original history.
//!
//! Captured references and read results carry no storage or routing authority.
//! Requested-byte counters are conservative cumulative ownership liabilities.

mod apply;
mod budget;
mod collect;
mod decode;
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
    history: &'a [HistoryValueEntry<'a, T>],
    value_return: HistoryValueReturn,
    limits: HistoryValueLimits,
) -> Result<HistoryValuePlan<'a, T>, HistoryValueError> {
    collect::prepare(history, value_return, limits)
}

/// Apply captured results as ordered writes without changing original entries.
///
/// # Errors
///
/// Returns malformed-batch, operational decode, resource or reservation errors.
pub fn apply_history_value_results<'a, T: ?Sized>(
    plan: HistoryValuePlan<'a, T>,
    batch: Option<HistoryValueReadBatch<'_>>,
    context: &OriginalValueContext<'_>,
) -> Result<HistoryValueOverlay<'a, T>, HistoryValueError> {
    apply::apply(plan, batch, context)
}
