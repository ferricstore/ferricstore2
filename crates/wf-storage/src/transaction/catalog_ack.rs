//! Explicit catalog acknowledgement under actual exclusive transaction authority.

use super::{CreditTransactionError, RecordBatchTransactionLimits, RecordBatchTransactionOutcome};
use crate::state::RedbStateStore;
use wf_domain::workflow::request::RequestScope;
use wf_types::version::LogIdentity;

mod driver;

/// Applies one verified ACK through the shared retained gate and atomic writer.
/// # Errors
/// Refuses unsupported/input/authority/resource failures; uncertainty fences.
pub fn apply_catalog_ack_transaction(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: RecordBatchTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, CreditTransactionError> {
    driver::apply(store, scope, encoded, log, limits)
}
