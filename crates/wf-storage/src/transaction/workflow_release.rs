//! Explicit GC4 successor of the stored-value workflow mutation driver.
use super::{
    RecordBatchTransactionOutcome, WorkflowValueTransactionError, WorkflowValueTransactionLimits,
};
use crate::state::RedbStateStore;
use wf_domain::workflow::request::RequestScope;
use wf_types::version::LogIdentity;

/// Applies workflow final writes together with their original release intents.
/// Acquisition stays in the separate Claim parent; profile7 is unsupported here.
/// # Errors
/// Refuses unsupported capabilities, authority, resources or unproved commit.
pub fn apply_workflow_release_transaction(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: WorkflowValueTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, WorkflowValueTransactionError> {
    super::value_batch::driver::apply_release(store, scope, encoded, log, limits)
}

pub(super) fn prepare<'a>(
    profile: super::value_batch::driver::ValueApplyProfile,
    tx: &crate::state::RedbStateTransaction<'_>,
    scope: &RequestScope,
    plan: wf_ports::log::RecordBatchApplyPlan<'a>,
    admission: wf_domain::workflow::admission::AdmissionLimits,
) -> Result<
    (
        wf_ports::log::RecordBatchApplyPlan<'a>,
        Option<crate::state::redb::release::ReleasePublicationPlan>,
    ),
    WorkflowValueTransactionError,
> {
    use crate::state::redb::release::{ReleasePreparation, prepare};
    if matches!(
        profile,
        super::value_batch::driver::ValueApplyProfile::Legacy
    ) {
        return Ok((plan, None));
    }
    match prepare(tx, scope, &plan).map_err(super::value_batch::storage_error)? {
        ReleasePreparation::Publish(release) => Ok((plan, Some(release))),
        ReleasePreparation::BacklogFull => Ok((
            wf_ports::log::reject_workflow_release_backlog(plan, admission)
                .map_err(|error| super::value_batch::record_error(error.into()))?,
            None,
        )),
    }
}
