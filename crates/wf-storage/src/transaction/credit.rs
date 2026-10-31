//! Atomic identified-owner driver and original uncertain-spend context.

use super::{RecordBatchTransactionLimits, RecordBatchTransactionOutcome};
use crate::state::RedbStateStore;
use std::fmt;
use wf_domain::governance::credit::{CreditError, CreditLimits};
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::CodecError;
use wf_ports::state::StateTransactionError;
use wf_types::version::LogIdentity;

mod driver;

impl CreditTransactionError {
    /// Maps only an uncertain original Spend to its exact compensation context.
    /// This projection establishes no commit, release or safe-retry authority.
    #[must_use]
    pub fn unknown_spend<'a>(
        self,
        original: &'a wf_ports::log::PreparedCreditRequest<'_>,
    ) -> Option<wf_domain::governance::view::CreditSpendUnknownView<'a>> {
        use wf_domain::governance::identified::IdentifiedCreditMutation;
        use wf_domain::governance::view::CreditSpendUnknownView;

        if self != Self::State(StateTransactionError::Uncertain) {
            return None;
        }
        let command = original.command();
        let IdentifiedCreditMutation::Spend {
            shard_id,
            reservation_ids,
            ..
        } = command.mutation
        else {
            return None;
        };
        Some(CreditSpendUnknownView {
            code: "FLOW_LIMIT_SPEND_UNKNOWN_OUTCOME",
            scope: command.scope,
            shard_id,
            reservation_ids,
            message: "Flow limit spend outcome is unknown; release these reservation_ids before retrying",
        })
    }
}

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
impl From<CodecError> for CreditTransactionError {
    fn from(error: CodecError) -> Self {
        Self::Codec(error)
    }
}
impl From<CreditError> for CreditTransactionError {
    fn from(error: CreditError) -> Self {
        Self::Credit(error)
    }
}
impl From<ApplyError> for CreditTransactionError {
    fn from(error: ApplyError) -> Self {
        Self::Apply(error)
    }
}
impl From<StateTransactionError> for CreditTransactionError {
    fn from(error: StateTransactionError) -> Self {
        Self::State(error)
    }
}

/// Applies one complete identified request under the real exclusive authority.
/// # Errors
/// Refuses unavailable/input/profile/authority failures; uncertainty fences.
pub fn apply_credit_transaction(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: RecordBatchTransactionLimits,
    credit: CreditLimits,
) -> Result<RecordBatchTransactionOutcome, CreditTransactionError> {
    driver::apply(store, scope, encoded, log, limits, credit)
}
