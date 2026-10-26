//! Captured inputs and ordinary plan metadata for history value projection.

use std::collections::TryReserveError;
use std::fmt;

use super::budget::HistoryValueCharge;
use crate::value::{OriginalFlowValueError, OriginalValueLimits};

/// Position of a value reference within one original history entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryValueKind {
    /// Payload reference, position zero.
    Payload,
    /// Result reference, position one.
    Result,
    /// Error reference, position two.
    Error,
}

/// Captured reference class; the complete original value remains in its source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryValueReference<'a> {
    /// An absent or nil reference.
    Nil,
    /// Original binary bytes, including the distinct empty binary.
    Binary(&'a [u8]),
    /// An ignored nonbinary reference class.
    NonBinary,
}

/// An unchanged original entry and its explicitly captured reference fields.
#[derive(Debug)]
pub struct HistoryValueEntry<'a, T: ?Sized> {
    /// Original fields and event identity, borrowed without reconstruction.
    pub source: &'a T,
    /// Corresponding payload, result and error reference classes in that order.
    pub references: [HistoryValueReference<'a>; 3],
}

/// Semantic stored-size selection, separate from operational resource limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryValueReturn {
    /// Preserve original entries without requesting value reads.
    Disabled,
    /// Return values whose complete stored length fits the signed maximum.
    Enabled {
        /// Signed source helper maximum; negative values omit every stored value.
        max_bytes: i128,
    },
}

/// One honestly captured result; this data does not acquire or authenticate it.
#[derive(Debug)]
pub enum HistoryValueReadResult<'read> {
    /// No value was returned for the reference.
    Missing,
    /// Captured storage failure, whose reason never enters projected fields.
    StorageFailure {
        /// Optional opaque diagnostic; projection never inspects or copies it.
        diagnostic: Option<&'read [u8]>,
    },
    /// Complete stored bytes owned by the captured read.
    Stored(Vec<u8>),
    /// An independently established stored-size classification.
    StoredSize {
        /// Complete stored byte count, including any wrapper.
        bytes: usize,
    },
    /// A source result class that performs no field write.
    Unrelated,
}

/// Captured list and eligibility restoration shape from the source reader.
#[derive(Debug)]
pub enum HistoryValueReadBatch<'read> {
    /// Direct post-reader values, zipped against collected references.
    Values(Vec<HistoryValueReadResult<'read>>),
    /// Captured eligibility mask and the values returned for eligible slots.
    Filtered {
        /// One captured eligibility flag per collected reference.
        eligibility: Vec<bool>,
        /// Results in eligible-reference order.
        values: Vec<HistoryValueReadResult<'read>>,
    },
    /// A malformed nonlist result, retained as an operational input error.
    NonList,
}

/// Positive operational bounds, independent of semantic stored-size omission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoryValueLimits {
    /// Maximum original entries visited.
    pub maximum_entries: usize,
    /// Maximum retained distinct references.
    pub maximum_references: usize,
    /// Maximum captured read results admitted before metadata traversal.
    pub maximum_read_results: usize,
    /// Maximum ordered field writes.
    pub maximum_writes: usize,
    /// Continuous work allowance across preparation and every decode attempt.
    pub maximum_work_units: usize,
    /// Conservative cumulative requested-owner liability, including read owners.
    pub maximum_requested_bytes: usize,
    /// Per-attempt decoder bounds, validated only when decoding is reached.
    pub decoder: OriginalValueLimits,
}

/// Borrowed original entries, stable references and one retained account.
#[derive(Debug)]
pub struct HistoryValuePlan<'a, T: ?Sized> {
    pub(super) history: &'a [HistoryValueEntry<'a, T>],
    pub(super) references: Vec<&'a [u8]>,
    pub(super) threshold: Option<usize>,
    pub(super) value_return: HistoryValueReturn,
    pub(super) limits: HistoryValueLimits,
    pub(super) charge: HistoryValueCharge,
}

impl<'a, T: ?Sized> HistoryValuePlan<'a, T> {
    /// Borrow the unchanged original entry views.
    #[must_use]
    pub fn history(&self) -> &'a [HistoryValueEntry<'a, T>] {
        self.history
    }

    /// Borrow stable first-occurrence references for the caller's real read.
    #[must_use]
    pub fn references(&self) -> &[&'a [u8]] {
        &self.references
    }

    /// Return the checked threshold; only disabled or empty history has none.
    #[must_use]
    pub fn read_threshold(&self) -> Option<usize> {
        self.threshold
    }

    /// Return the captured semantic selection.
    #[must_use]
    pub fn value_return(&self) -> HistoryValueReturn {
        self.value_return
    }

    /// Return the operational profile attached to the same account.
    #[must_use]
    pub fn limits(&self) -> HistoryValueLimits {
        self.limits
    }

    /// Return conservatively admitted work cohorts, including full decode caps.
    #[must_use]
    pub fn charged_work(&self) -> usize {
        self.charge.work_units
    }

    /// Return cumulative requested-owner liabilities, rather than live heap use.
    #[must_use]
    pub fn charged_requested_bytes(&self) -> usize {
        self.charge.requested_bytes
    }
}

/// Malformed input and operational refusals, distinct from projected corruption.
#[derive(Debug)]
pub enum HistoryValueError {
    /// An outer operational bound is zero.
    InvalidLimits,
    /// Checked arithmetic or a positive operational bound refused the operation.
    Limit,
    /// The supplied captured batch does not match the requested read shape.
    InvalidReadBatch,
    /// An unchanged operational decoder error, retaining its exact details.
    Decode(OriginalFlowValueError),
    /// An actual fallible reservation failed.
    ReserveFailed(TryReserveError),
}

impl fmt::Display for HistoryValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLimits => "invalid history value limits",
            Self::Limit => "history value resource limit",
            Self::InvalidReadBatch => "invalid history value read batch",
            Self::Decode(_) => "history value decoding failed",
            Self::ReserveFailed(_) => "history value reservation failed",
        })
    }
}

impl std::error::Error for HistoryValueError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReserveFailed(error) => Some(error),
            _ => None,
        }
    }
}
