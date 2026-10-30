//! Identified-owner driver contract; unavailable until atomic integration.

use super::{RecordBatchTransactionLimits, RecordBatchTransactionOutcome};
use crate::state::RedbStateStore;
use std::fmt;
use wf_domain::governance::credit::{CreditError, CreditLimits};
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::CodecError;
use wf_ports::state::StateTransactionError;
use wf_types::version::LogIdentity;

/// Operational failure; never a retained business rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditTransactionError {
    /// Invalid trusted profile.
    InvalidLimits,
    /// Private framing/input/resource failure.
    Codec(CodecError),
    /// Canonical capacity/resource failure.
    Credit(CreditError),
    /// Ordered request/control authority failure.
    Apply(ApplyError),
    /// Actual storage/captured-profile/durability failure.
    State(StateTransactionError),
}
impl fmt::Display for CreditTransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => f.write_str("invalid credit transaction limits"),
            Self::Codec(e) => fmt::Display::fmt(e, f),
            Self::Credit(e) => fmt::Display::fmt(e, f),
            Self::Apply(e) => fmt::Display::fmt(e, f),
            Self::State(e) => fmt::Display::fmt(e, f),
        }
    }
}
impl std::error::Error for CreditTransactionError {}

/// Applies one complete identified request under the real exclusive authority.
/// # Errors
/// Refuses unavailable/input/profile/authority failures; uncertainty fences.
pub fn apply_credit_transaction(
    _store: &mut RedbStateStore,
    _scope: &RequestScope,
    _encoded: &[u8],
    _log: LogIdentity,
    _limits: RecordBatchTransactionLimits,
    _credit: CreditLimits,
) -> Result<RecordBatchTransactionOutcome, CreditTransactionError> {
    Err(CreditTransactionError::State(
        StateTransactionError::UnsupportedLayout,
    ))
}
