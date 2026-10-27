//! Consuming observation conversion, without acquisition or query authority.

use std::collections::TryReserveError;
use std::fmt;
use std::mem::size_of;
use wf_domain::history::values::{HistoryValueReadBatch, HistoryValueReadResult};

use super::{WorkflowValueReadFailure, WorkflowValueSizedReadBatch, WorkflowValueSizedReadValue};

use HistoryValueReadConversionError as E;

/// Independent conversion bounds; backend and domain accounts stay separate.
#[derive(Clone, Copy, Debug)]
pub struct HistoryValueReadConversionLimits {
    /// Maximum unique references represented by the ordered input slots.
    pub maximum_references: usize,
    /// Maximum logical conversion work, excluding backend and decoder work.
    pub maximum_work: usize,
    /// Incoming capacities plus requested result spine and typed fixed metadata.
    pub maximum_temporary_bytes: usize,
}

/// Malformed observations and operational conversion refusals.
#[derive(Debug)]
pub enum HistoryValueReadConversionError {
    /// At least one conversion bound is zero.
    InvalidLimits,
    /// A count, logical work or temporary ownership ceiling was exceeded.
    Limit,
    /// Slots do not consume each successful unique-reference owner in order.
    InvalidBatch,
    /// This observation cannot honestly become a missing value or field error.
    Unprojectable {
        /// Original stable unique-reference position.
        reference_index: usize,
        /// Exact observed refusal, without a private storage diagnostic.
        failure: WorkflowValueReadFailure,
    },
    /// A real fallible result-spine reservation failed.
    ReserveFailed(TryReserveError),
}

impl fmt::Display for HistoryValueReadConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLimits => "invalid history read conversion limits",
            Self::Limit => "history read conversion resource limit",
            Self::InvalidBatch => "invalid history read observations",
            Self::Unprojectable { .. } => "history read observation cannot be projected",
            Self::ReserveFailed(_) => "history read conversion reservation failed",
        })
    }
}

impl std::error::Error for HistoryValueReadConversionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReserveFailed(error) => Some(error),
            _ => None,
        }
    }
}

/// Consume observations for a collector's stable unique-reference list.
///
/// This ordinary conversion does not acquire or authenticate values. Stored
/// buffers move unchanged; the domain still funds each reached decode copy.
/// The conversion profile is independent of backend and domain profiles.
///
/// # Errors
/// Rejects invalid limits, malformed owner sequences, unprojectable observations,
/// resource exhaustion and actual reservation failure.
pub fn convert_history_value_reads(
    batch: WorkflowValueSizedReadBatch,
    reference_count: usize,
    limits: HistoryValueReadConversionLimits,
) -> Result<HistoryValueReadBatch<'static>, HistoryValueReadConversionError> {
    // Fixed bootstrap only; no input traversal or allocation precedes admission.
    if [
        limits.maximum_references,
        limits.maximum_work,
        limits.maximum_temporary_bytes,
    ]
    .contains(&0)
    {
        return Err(E::InvalidLimits);
    }
    let mut work = 0;
    // Eight scalar cohorts fund sizing, validation and iterator/result setup.
    spend(&mut work, 1_024, limits.maximum_work)?;
    if reference_count > limits.maximum_references {
        return Err(E::Limit);
    }
    if batch.slots.len() != reference_count || batch.values.len() > reference_count {
        return Err(E::InvalidBatch);
    }
    let live = input_bytes(&batch, limits, &mut work)?;
    let requested = product(reference_count, size_of::<HistoryValueReadResult<'_>>())?;
    temporary(add(live, requested)?, limits.maximum_temporary_bytes)?;
    spend(&mut work, 128, limits.maximum_work)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(reference_count)
        .map_err(E::ReserveFailed)?;
    spend(
        &mut work,
        product(reference_count, 128)?,
        limits.maximum_work,
    )?;
    let mut values = batch.values.into_iter();
    let mut owner = 0;
    for (reference_index, slot) in batch.slots.into_iter().enumerate() {
        let value = match slot {
            Ok(Some(index)) => {
                if index != owner {
                    return Err(E::InvalidBatch);
                }
                let value = values.next().ok_or(E::InvalidBatch)?;
                owner = add(owner, 1)?;
                match value {
                    WorkflowValueSizedReadValue::Stored(bytes) => {
                        HistoryValueReadResult::Stored(bytes)
                    }
                    WorkflowValueSizedReadValue::Omitted { stored_bytes } => {
                        HistoryValueReadResult::StoredSize {
                            bytes: stored_bytes,
                        }
                    }
                }
            }
            Ok(None) => HistoryValueReadResult::Missing,
            Err(WorkflowValueReadFailure::ReadFailed | WorkflowValueReadFailure::Unavailable) => {
                HistoryValueReadResult::StorageFailure { diagnostic: None }
            }
            Err(failure) => {
                return Err(E::Unprojectable {
                    reference_index,
                    failure,
                });
            }
        };
        output.push(value);
    }
    spend(&mut work, 128, limits.maximum_work)?;
    if values.next().is_some() {
        return Err(E::InvalidBatch);
    }
    Ok(HistoryValueReadBatch::Values(output))
}

fn input_bytes(
    batch: &WorkflowValueSizedReadBatch,
    limits: HistoryValueReadConversionLimits,
    work: &mut usize,
) -> Result<usize, E> {
    let mut live = add(
        fixed_bytes(),
        add(
            product(
                batch.values.capacity(),
                size_of::<WorkflowValueSizedReadValue>(),
            )?,
            product(
                batch.slots.capacity(),
                size_of::<Result<Option<usize>, WorkflowValueReadFailure>>(),
            )?,
        )?,
    )?;
    temporary(live, limits.maximum_temporary_bytes)?;
    spend(work, product(batch.values.len(), 128)?, limits.maximum_work)?;
    for value in &batch.values {
        if let WorkflowValueSizedReadValue::Stored(bytes) = value {
            live = add(live, bytes.capacity())?;
            temporary(live, limits.maximum_temporary_bytes)?;
        }
    }
    Ok(live)
}

fn fixed_bytes() -> usize {
    // Phase-summed headers and twelve scalar cells, not a native-frame theorem.
    size_of::<(
        WorkflowValueSizedReadBatch,
        Result<HistoryValueReadBatch<'static>, E>,
        Vec<HistoryValueReadResult<'static>>,
        std::vec::IntoIter<WorkflowValueSizedReadValue>,
        std::iter::Enumerate<std::vec::IntoIter<Result<Option<usize>, WorkflowValueReadFailure>>>,
        std::slice::Iter<'static, WorkflowValueSizedReadValue>,
        HistoryValueReadConversionLimits,
        Result<Option<usize>, WorkflowValueReadFailure>,
        Option<WorkflowValueSizedReadValue>,
        HistoryValueReadResult<'static>,
        [usize; 12],
    )>()
}

fn spend(work: &mut usize, units: usize, maximum: usize) -> Result<(), E> {
    let next = add(*work, units)?;
    if next > maximum {
        return Err(E::Limit);
    }
    *work = next;
    Ok(())
}

fn temporary(bytes: usize, maximum: usize) -> Result<(), E> {
    if bytes > maximum {
        return Err(E::Limit);
    }
    Ok(())
}

fn add(left: usize, right: usize) -> Result<usize, E> {
    left.checked_add(right).ok_or(E::Limit)
}

fn product(count: usize, width: usize) -> Result<usize, E> {
    count.checked_mul(width).ok_or(E::Limit)
}
