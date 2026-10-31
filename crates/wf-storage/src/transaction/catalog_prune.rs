//! Derived progress dispatch with original retained/control atomicity.
use super::{RecordBatchTransactionLimits, RecordBatchTransactionOutcome};
use crate::state::RedbStateStore;
use std::fmt;
use wf_domain::workflow::{batch::ApplyError, request::RequestScope};
use wf_ports::{log::CodecError, state::StateTransactionError};
use wf_types::version::LogIdentity;

mod driver;

/// Operational failures; none is a retained success or business rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogPruneTransactionError {
    /// Invalid trusted positive profiles.
    InvalidLimits,
    /// Complete framing or conversion admission failed.
    Codec(CodecError),
    /// Ordered retained/control authority was invalid.
    Apply(ApplyError),
    /// State, admission or real durability failed.
    State(StateTransactionError),
}
impl fmt::Display for CatalogPruneTransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => f.write_str("invalid catalog prune limits"),
            Self::Codec(e) => e.fmt(f),
            Self::Apply(e) => e.fmt(f),
            Self::State(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for CatalogPruneTransactionError {}
/// Applies one complete original progress command through a consumed writer.
/// # Errors
/// Refuses framing/authority/resources and uncertain commit durability.
pub fn apply_catalog_prune_transaction(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: RecordBatchTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, CatalogPruneTransactionError> {
    driver::apply(store, scope, encoded, log, limits)
}

impl From<CodecError> for CatalogPruneTransactionError {
    fn from(error: CodecError) -> Self {
        Self::Codec(error)
    }
}
impl From<ApplyError> for CatalogPruneTransactionError {
    fn from(error: ApplyError) -> Self {
        Self::Apply(error)
    }
}
impl From<StateTransactionError> for CatalogPruneTransactionError {
    fn from(error: StateTransactionError) -> Self {
        Self::State(error)
    }
}
