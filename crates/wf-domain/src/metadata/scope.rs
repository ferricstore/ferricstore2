//! Original sealed scope and partition preparation, without placement authority.
use super::OriginalMetadataInput;
use std::borrow::Cow;
mod arena;
mod auto;
mod budget;
mod record;
mod system;
use OriginalScopeError as E;
use budget::Budget;

/// Independent positive operational ceilings, separate from source validity.
#[derive(Clone, Copy, Debug)]
pub struct OriginalScopeLimits {
    /// Maximum returned binary partition or scope bytes.
    pub maximum_output_bytes: usize,
    /// Maximum continuous charged work.
    pub maximum_work_units: usize,
    /// Conservative peak requested temporary ownership, excluding caller inputs.
    pub maximum_temporary_bytes: usize,
}

/// Source invalidity and operational refusals remain distinguishable.
#[derive(Debug)]
pub enum OriginalScopeError {
    /// Source `:invalid_flow_system_metadata`.
    InvalidSystemMetadata,
    /// A required operational ceiling is zero.
    InvalidLimits,
    /// A checked operational ceiling was exceeded.
    Limit,
    /// A fallible reservation failed.
    ReserveFailed(std::collections::TryReserveError),
    /// The shared ETF encoder refused preparation.
    Encoding(crate::value::OriginalValueEncodeError),
}

/// Ordinary data and its accounting; this grants no validation authority.
#[derive(Debug)]
pub struct OriginalScopeResult<T> {
    /// Prepared ordinary data.
    pub value: T,
    /// Continuous charged work through this operation.
    pub work_units: usize,
    /// Conservative peak of requested logical owners, excluding caller inputs.
    pub temporary_bytes: usize,
}

/// Measured source partition success or invalid metadata, before placement.
pub type OriginalRecordPartitionObservation<'a> =
    OriginalScopeResult<Result<Option<Cow<'a, [u8]>>, ()>>;

/// Derive the original validated isolation scope from a captured metadata map.
/// # Errors
/// Distinguishes invalid source metadata, operational ceilings and allocation.
pub fn original_system_metadata_scope(
    metadata: OriginalMetadataInput<'_, '_>,
    limits: OriginalScopeLimits,
) -> Result<OriginalScopeResult<Option<Vec<u8>>>, OriginalScopeError> {
    let mut budget = Budget::new(limits)?;
    let value = system::run(metadata, &mut budget)?;
    Ok(budget.finish(value))
}

/// Apply the original direct physical-partition overload.
/// # Errors
/// Distinguishes invalid source operands and operational refusal.
pub fn original_physical_partition<'a>(
    partition: &'a [u8],
    scope: Option<&[u8]>,
    limits: OriginalScopeLimits,
) -> Result<OriginalScopeResult<Cow<'a, [u8]>>, E> {
    let mut budget = Budget::new(limits)?;
    if partition.is_empty() {
        return Err(E::InvalidSystemMetadata);
    }
    let value = encode(partition, scope, &mut budget)?;
    Ok(budget.finish(value))
}

/// Apply the original direct logical-partition overload.
/// # Errors
/// Refuses empty/mismatched scope and operational ceilings.
pub fn original_logical_partition<'a>(
    partition: &'a [u8],
    scope: &[u8],
    limits: OriginalScopeLimits,
) -> Result<OriginalScopeResult<&'a [u8]>, E> {
    let mut budget = Budget::new(limits)?;
    if scope.is_empty() {
        return Err(E::InvalidSystemMetadata);
    }
    let value = decode(partition, scope, &mut budget)?;
    budget.output(value.len())?;
    Ok(budget.finish(value))
}

/// Inspect a physical scope; malformed or empty-tail inputs are unscoped.
#[must_use]
pub fn original_physical_scope(partition: &[u8]) -> Option<&[u8]> {
    let (scope, logical) = split(partition)?;
    if scope.is_empty() || scope.len() > 8192 || logical.is_empty() {
        return None;
    }
    Some(scope)
}

/// Recognize the exact ordered set of 256 scoped automatic partitions.
/// # Errors
/// Invalid source sets and independent operational ceilings refuse.
pub fn original_scoped_auto_partition_scope<'a>(
    partitions: &[&'a [u8]],
    limits: OriginalScopeLimits,
) -> Result<OriginalScopeResult<&'a [u8]>, E> {
    let mut budget = Budget::new(limits)?;
    let value = auto::run(partitions, &mut budget)?;
    Ok(budget.finish(value))
}

/// Apply the original record physical overload, preserving absent/nil/binary.
/// # Errors
/// Invalid captured records and independent operational ceilings refuse.
pub fn original_record_physical_partition<'a>(
    partition: Option<OriginalMetadataInput<'_, 'a>>,
    metadata: Option<OriginalMetadataInput<'_, '_>>,
    limits: OriginalScopeLimits,
) -> Result<OriginalScopeResult<Option<Cow<'a, [u8]>>>, E> {
    let mut budget = Budget::new(limits)?;
    let value = record::run(partition, metadata, false, &mut budget)?;
    Ok(budget.finish(value))
}

/// Observe the original record physical overload, including source invalidity.
///
/// This observation preserves charged work for source-invalid records too.
/// # Errors
/// Operational ceilings and failed reservations remain errors; source invalidity
/// is the measured inner `Err(())` result.
pub fn original_record_physical_partition_observation<'a>(
    partition: Option<OriginalMetadataInput<'_, 'a>>,
    metadata: Option<OriginalMetadataInput<'_, '_>>,
    limits: OriginalScopeLimits,
) -> Result<OriginalRecordPartitionObservation<'a>, E> {
    let mut budget = Budget::new(limits)?;
    let value = match record::run(partition, metadata, false, &mut budget) {
        Ok(value) => Ok(value),
        Err(E::InvalidSystemMetadata) => Err(()),
        Err(error) => return Err(error),
    };
    Ok(budget.finish(value))
}

/// Apply the original record logical overload.
/// # Errors
/// Invalid captured records, mismatched scopes and operational ceilings refuse.
pub fn original_record_logical_partition<'a>(
    partition: Option<OriginalMetadataInput<'_, 'a>>,
    metadata: Option<OriginalMetadataInput<'_, '_>>,
    limits: OriginalScopeLimits,
) -> Result<OriginalScopeResult<Option<Cow<'a, [u8]>>>, E> {
    let mut budget = Budget::new(limits)?;
    let value = record::run(partition, metadata, true, &mut budget)?;
    Ok(budget.finish(value))
}

const PREFIX: &[u8] = b"\0\xfeflow-scope\0";

fn split(partition: &[u8]) -> Option<(&[u8], &[u8])> {
    let rest = partition.strip_prefix(PREFIX)?;
    let length = usize::from(u16::from_be_bytes(rest.get(..2)?.try_into().ok()?));
    let end = 2_usize.checked_add(length)?;
    Some((rest.get(2..end)?, rest.get(end..)?))
}

fn decode<'a>(partition: &'a [u8], scope: &[u8], budget: &mut Budget) -> Result<&'a [u8], E> {
    budget.spend(budget::add(scope.len(), 16)?)?;
    let (encoded, logical) = split(partition).ok_or(E::InvalidSystemMetadata)?;
    if encoded != scope {
        return Err(E::InvalidSystemMetadata);
    }
    Ok(logical)
}

fn encode<'a>(
    partition: &'a [u8],
    scope: Option<&[u8]>,
    budget: &mut Budget,
) -> Result<Cow<'a, [u8]>, E> {
    let Some(scope) = scope else {
        budget.output(partition.len())?;
        return Ok(Cow::Borrowed(partition));
    };
    if scope.is_empty() || scope.len() > 8192 {
        return Err(E::InvalidSystemMetadata);
    }
    match decode(partition, scope, budget) {
        Ok(_) => {
            budget.output(partition.len())?;
            return Ok(Cow::Borrowed(partition));
        }
        Err(E::InvalidSystemMetadata) => {}
        Err(error) => return Err(error),
    }
    let length = budget::add(budget::add(PREFIX.len() + 2, scope.len())?, partition.len())?;
    budget.output(length)?;
    let mut bytes = budget.reserve(length)?;
    bytes.extend_from_slice(PREFIX);
    bytes.extend_from_slice(
        &u16::try_from(scope.len())
            .map_err(|_| E::Limit)?
            .to_be_bytes(),
    );
    bytes.extend_from_slice(scope);
    bytes.extend_from_slice(partition);
    Ok(Cow::Owned(bytes))
}
