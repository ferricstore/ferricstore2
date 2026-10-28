//! Explicit logical value preparation for the existing mutation vocabulary.

mod budget;
mod capture;
mod materialize;
mod model;
mod values;

pub use model::{
    LogicalMutationError, LogicalMutationFieldValue, LogicalMutationInput, LogicalMutationLimits,
    PreparedLogicalMutation,
};

use wf_types::version::CommittedTime;

use super::{MutationInput, normalize_core_mutation};
use budget::{Budget, add, mul};

/// Own a normalized caller and its complete source-compatible stored values.
///
/// The result is intended for explicit stored-value intent9 construction.
/// Supplied time, partition and captures grant no origin, routing or admission
/// authority. No clock, storage or runtime lookup is performed.
///
/// # Errors
///
/// Caller, value-capture and operational failures retain separate error lanes.
/// All original occurrences are bounded before scalar normalization or logical
/// traversal. Failed work is not refunded and no partial command is returned.
pub fn prepare_logical_mutation(
    input: LogicalMutationInput<'_>,
    common: &[(&str, LogicalMutationFieldValue<'_>)],
    base_partition: Option<&[u8]>,
    stamp: CommittedTime,
    limits: LogicalMutationLimits,
) -> Result<PreparedLogicalMutation, LogicalMutationError> {
    let mut budget = Budget::new(limits)?;
    let raw = capture::measure(input, common, base_partition, limits.max_raw_bytes)?;
    // Finite field slices bound pairwise name checks. Admit their conservative
    // repeated inspection liability before executing the existing normalizer.
    budget.spend(add(mul(raw, 32)?, 256)?)?;
    let fields = capture::project(input.operation, input.fields);
    let inherited = capture::project(input.operation, common);
    let caller = normalize_core_mutation(
        MutationInput {
            operation: input.operation,
            id: input.id,
            fields: &fields[..input.fields.len()],
        },
        &inherited[..common.len()],
        base_partition,
        limits.admission,
        limits.max_raw_bytes,
    )?;
    budget.metadata(&caller)?;
    let values = values::prepare(input, common, &mut budget)?;
    materialize::finish(&caller, values, stamp, &budget)
}
