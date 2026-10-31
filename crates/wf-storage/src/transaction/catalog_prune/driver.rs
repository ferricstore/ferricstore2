//! Rejecting selected writer; no fabricated atomicity.
use super::*;
pub(super) fn apply(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: RecordBatchTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, CatalogPruneTransactionError> {
    let _ = (store, scope, encoded, log, limits);
    Err(StateTransactionError::UnsupportedPolicyProfile.into())
}
