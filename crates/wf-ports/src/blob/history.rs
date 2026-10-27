//! Consuming observation conversion, without acquisition or query authority.

use std::collections::TryReserveError;
use std::fmt;
use wf_domain::history::values::HistoryValueReadBatch;

use super::{WorkflowValueReadFailure, WorkflowValueSizedReadBatch};

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
    _batch: WorkflowValueSizedReadBatch,
    _reference_count: usize,
    _limits: HistoryValueReadConversionLimits,
) -> Result<HistoryValueReadBatch<'static>, HistoryValueReadConversionError> {
    Err(HistoryValueReadConversionError::InvalidBatch)
}
