//! Original FSH2 per-event bytes, separate from authoritative history state.

mod budget;
mod decode;
mod encode;
mod model;
mod number;
mod primitives;
mod project;

pub use model::{
    DecodedOriginalCompactHistory, OriginalCompactHistoryContext, OriginalCompactHistoryError,
    OriginalCompactHistoryField, OriginalCompactHistoryLimits, OriginalCompactHistorySnapshot,
    PreparedOriginalCompactHistory,
};

/// Prepare the original compact format from explicit captured snapshot values.
///
/// Ordered metadata comes from the existing normalizer. Native record capture,
/// JSON sidecar injection, history authority and durable publication stay outer.
///
/// # Errors
/// Invalid captures/profiles, operational limits and failed reservations differ.
pub fn prepare_original_compact_history(
    snapshot: &OriginalCompactHistorySnapshot<'_, '_>,
    limits: OriginalCompactHistoryLimits,
) -> Result<PreparedOriginalCompactHistory, OriginalCompactHistoryError> {
    encode::run(snapshot, limits)
}

/// Decode the complete frame and restore its omitted immutable context fields.
///
/// Malformed wire yields empty fields. Surviving byte fields borrow their input;
/// numeric decimal text is owned. No record, read or persistence authority is made.
///
/// # Errors
/// Invalid limits, operational resources and actual reservations remain errors.
pub fn decode_original_compact_history<'a>(
    bytes: &'a [u8],
    context: OriginalCompactHistoryContext<'a>,
    limits: OriginalCompactHistoryLimits,
) -> Result<DecodedOriginalCompactHistory<'a>, OriginalCompactHistoryError> {
    decode::run(bytes, context, limits)
}
