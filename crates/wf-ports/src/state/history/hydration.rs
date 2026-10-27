//! Owned physical history and source field writes, without public query authority.

use std::fmt;

use wf_domain::history::values::{
    HistoryValueError, HistoryValueLimits, HistoryValueReturn, HistoryValueWrite,
};
use wf_domain::value::OriginalValueContext;

use super::{WorkflowHistoryPage, WorkflowHistoryQuery, WorkflowHistoryReadLimits};
use crate::blob::{
    HistoryValueReadConversionError, HistoryValueReadConversionLimits, WorkflowValueReadLimits,
};
use crate::state::StateTransactionError;

/// Physical selection, semantic value return and explicit decoder context.
#[derive(Clone, Copy, Debug)]
pub struct WorkflowHistoryHydrationRequest<'a> {
    /// Selected physical rows and owned lookahead.
    pub query: WorkflowHistoryQuery<'a>,
    /// Disabled or signed stored-size selection.
    pub value_return: HistoryValueReturn,
    /// Immutable compatibility observations; no runtime authority is inferred.
    pub decode_context: OriginalValueContext<'a>,
}

/// Independent reached-stage ceilings and whole-composition logical admission.
#[derive(Clone, Copy)]
pub struct WorkflowHistoryHydrationLimits {
    /// Physical metadata acquisition.
    pub history: WorkflowHistoryReadLimits,
    /// Authenticated sized value acquisition on the same snapshot.
    pub values: WorkflowValueReadLimits,
    /// Consuming captured-read conversion.
    pub conversion: HistoryValueReadConversionLimits,
    /// Stable reference collection and every per-occurrence decode/write.
    pub projection: HistoryValueLimits,
    /// Nonrefundable reached-stage work cohorts and adapter work.
    pub maximum_work: usize,
    /// Simultaneous named ownership plus conservative reached-stage bounds.
    pub maximum_temporary_bytes: usize,
}

/// Physical candidates with exact owned field writes, including lookahead.
pub struct WorkflowHistoryValuePage {
    /// Coherent metadata and original binding provenance.
    pub history: WorkflowHistoryPage,
    /// Indices address selected entries followed by the optional lookahead.
    pub writes: Vec<HistoryValueWrite>,
    /// Pure projector's continuous work account; excludes adapter/backend work.
    pub projection_work_units: usize,
    /// Pure projector's cumulative requested-owner liabilities.
    pub projection_requested_bytes: usize,
}

/// Structured stage refusals, separate from sanitized per-field source errors.
#[derive(Debug)]
pub enum WorkflowHistoryHydrationError {
    /// Physical state, stored-envelope, admission or engine refusal.
    State(StateTransactionError),
    /// Clock refusal alone does not fence the store.
    Clock(StateTransactionError),
    /// Captured read refusal that cannot honestly be projected as missing.
    Conversion(HistoryValueReadConversionError),
    /// Reference collection, reached decoder or projection resource refusal.
    Projection(HistoryValueError),
}

impl fmt::Display for WorkflowHistoryHydrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::State(_) => "history state read failed",
            Self::Clock(_) => "history clock read failed",
            Self::Conversion(_) => "history read conversion failed",
            Self::Projection(_) => "history value projection failed",
        })
    }
}

impl std::error::Error for WorkflowHistoryHydrationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::State(error) | Self::Clock(error) => error,
            Self::Conversion(error) => error,
            Self::Projection(error) => error,
        })
    }
}

impl From<StateTransactionError> for WorkflowHistoryHydrationError {
    fn from(error: StateTransactionError) -> Self {
        Self::State(error)
    }
}

impl From<HistoryValueReadConversionError> for WorkflowHistoryHydrationError {
    fn from(error: HistoryValueReadConversionError) -> Self {
        Self::Conversion(error)
    }
}

impl From<HistoryValueError> for WorkflowHistoryHydrationError {
    fn from(error: HistoryValueError) -> Self {
        Self::Projection(error)
    }
}
