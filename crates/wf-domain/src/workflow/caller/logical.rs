//! Explicit logical value preparation for the existing mutation vocabulary.

mod model;

pub use model::{
    LogicalMutationError, LogicalMutationFieldValue, LogicalMutationInput, LogicalMutationLimits,
    PreparedLogicalMutation,
};

use wf_types::version::CommittedTime;

use super::MutationCallerError;

/// Own a normalized caller and its complete source-compatible stored values.
///
/// The result is intended for explicit stored-value intent9 construction.
/// Supplied time, partition and captures grant no origin, routing or admission
/// authority. No clock, storage or runtime lookup is performed.
///
/// # Errors
///
/// This rejecting TDD facade is not implemented. Caller, value-capture and
/// operational failures must retain separate error lanes.
pub fn prepare_logical_mutation(
    _input: LogicalMutationInput<'_>,
    _common: &[(&str, LogicalMutationFieldValue<'_>)],
    _base_partition: Option<&[u8]>,
    _stamp: CommittedTime,
    _limits: LogicalMutationLimits,
) -> Result<PreparedLogicalMutation, LogicalMutationError> {
    Err(LogicalMutationError::Caller(
        MutationCallerError::UnsupportedFeature("logical_value"),
    ))
}
