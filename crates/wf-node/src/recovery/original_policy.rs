//! Complete borrowed original history audit and actual opt-in driver replay.

use super::{CommittedRecoveryError as Error, CommittedRecoveryLimits, total};
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_domain::workflow::control::EntryStamp;
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::{RaftLogEntry, prepare_original_policy_set_point_frame_bounded};
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    OriginalPolicyTransactionError, RecordBatchTransactionLimits, RecordBatchTransactionOutcome,
    apply_original_policy_set_transaction,
};
use wf_types::version::CommittedTime;

impl CommittedRecoveryLimits {
    /// Explicitly enables original public Policy.set replay on the normal profile.
    #[must_use]
    pub const fn with_original_policy_sets(mut self) -> Self {
        self.original_policy = true;
        self
    }
    /// Reads the explicit opt-in; legacy construction leaves it disabled.
    #[must_use]
    pub const fn original_policy_sets(self) -> bool {
        self.original_policy
    }
}

pub(super) fn audit(
    body: &[u8],
    limits: &CommittedRecoveryLimits,
    base: usize,
    stamp: CommittedTime,
) -> Result<EntryStamp, Error> {
    if !limits.original_policy {
        return Err(Error::UnsupportedEntry);
    }
    let driver = limits.normal;
    let pg = usize::try_from(driver.schema.max_work_bytes()).map_err(|_| Error::Limit)?;
    let header = body
        .len()
        .checked_mul(3)
        .and_then(|n| n.checked_add(2048))
        .ok_or(Error::Limit)?;
    limits.admit(&[base, 512, pg, 65_536, header])?;
    let available = limits
        .workspace
        .checked_sub(total(&[base, 512])?)
        .ok_or(Error::Limit)?;
    let mut charge = PolicySchemaCharge::new(driver.schema);
    let verified = prepare_original_policy_set_point_frame_bounded(
        body,
        driver.schema,
        driver.codec.admission(),
        &mut charge,
        available,
    )
    .map_err(|error| Error::OriginalPolicy(OriginalPolicyTransactionError::Original(error)))?;
    // No point gate, Patch/Plan owner, current lookup or replay pool is created.
    drop(verified);
    Ok(EntryStamp::Stamped(stamp))
}

pub(super) fn apply(
    state: &mut RedbStateStore,
    scope: &RequestScope,
    entry: &RaftLogEntry,
    limits: RecordBatchTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, Error> {
    let mut charge = PolicySchemaCharge::new(limits.schema);
    apply_original_policy_set_transaction(
        state,
        scope,
        entry.body(),
        entry.id(),
        limits,
        &mut charge,
    )
    .map_err(Error::OriginalPolicy)
}
