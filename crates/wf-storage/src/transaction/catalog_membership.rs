//! Private catalog membership dispatch under the existing retained/control gate.

use super::{RecordBatchTransactionLimits, RecordBatchTransactionOutcome};
use crate::state::RedbStateStore;
use std::fmt;
use wf_domain::workflow::{batch::ApplyError, request::RequestScope};
use wf_ports::{log::CodecError, state::StateTransactionError};
use wf_types::version::LogIdentity;

/// Operational failures never become a retained catalog business outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogMembershipTransactionError {
    /// Required positive trusted limits are absent.
    InvalidLimits,
    /// Original framing, scalar or decoder admission failed.
    Codec(CodecError),
    /// Ordered retained/control authority is incoherent.
    Apply(ApplyError),
    /// Actual state, admission or durable commit failed.
    State(StateTransactionError),
}
impl fmt::Display for CatalogMembershipTransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => f.write_str("invalid catalog membership limits"),
            Self::Codec(e) => fmt::Display::fmt(e, f),
            Self::Apply(e) => fmt::Display::fmt(e, f),
            Self::State(e) => fmt::Display::fmt(e, f),
        }
    }
}
impl std::error::Error for CatalogMembershipTransactionError {}
impl From<CodecError> for CatalogMembershipTransactionError {
    fn from(e: CodecError) -> Self {
        Self::Codec(e)
    }
}
impl From<ApplyError> for CatalogMembershipTransactionError {
    fn from(e: ApplyError) -> Self {
        Self::Apply(e)
    }
}
impl From<StateTransactionError> for CatalogMembershipTransactionError {
    fn from(e: StateTransactionError) -> Self {
        Self::State(e)
    }
}

/// Applies one complete command with its original reply and control atomically.
/// # Errors
/// Refuses framing/authority/resources; uncertain commit fences the live owner.
pub fn apply_catalog_membership_transaction(
    _store: &mut RedbStateStore,
    _scope: &RequestScope,
    _encoded: &[u8],
    _log: LogIdentity,
    _limits: RecordBatchTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, CatalogMembershipTransactionError> {
    Err(StateTransactionError::UnsupportedPolicyProfile.into())
}
