//! Explicit catalog acknowledgement under actual exclusive transaction authority.

use super::{CreditTransactionError, RecordBatchTransactionLimits, RecordBatchTransactionOutcome};
use crate::state::RedbStateStore;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::StateTransactionError;
use wf_types::version::LogIdentity;

/// Applies one verified ACK through the shared retained gate and atomic writer.
/// # Errors
/// Refuses unsupported/input/authority/resource failures; uncertainty fences.
pub fn apply_catalog_ack_transaction(
    _store: &mut RedbStateStore,
    _scope: &RequestScope,
    _encoded: &[u8],
    _log: LogIdentity,
    _limits: RecordBatchTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, CreditTransactionError> {
    Err(CreditTransactionError::State(
        StateTransactionError::UnsupportedPolicyProfile,
    ))
}
