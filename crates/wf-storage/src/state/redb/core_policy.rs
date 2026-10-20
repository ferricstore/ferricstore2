//! Same-writer complete-policy extension and strictly captured guard context.

use super::{commit, store::RedbStateTransaction};
use wf_domain::policy::complete::{PolicySchemaCharge, PolicySchemaLimits};
use wf_domain::workflow::request::RetainedOutcome;
use wf_ports::state::{GroupCorePolicyChanges, GroupCorePolicyTransaction, StateTransactionError};
use wf_types::identity::{RequestId, TenantId};

pub(super) mod catalog;
pub(super) mod reindex;

impl GroupCorePolicyTransaction for RedbStateTransaction<'_> {
    fn core_policy_schema_limits(&self) -> PolicySchemaLimits {
        self.limits.schema()
    }
    fn core_policy_history_max_events_limit(&self) -> u32 {
        self.limits.history_max_events_limit()
    }
    fn core_policy_layout(&self) -> Result<u16, StateTransactionError> {
        self.ready()?;
        if self.recovering || self.snapshot_cohort {
            return Err(StateTransactionError::InvalidChanges);
        }
        if !matches!(self.layout, 3..=5) {
            return Err(StateTransactionError::UnsupportedLayout);
        }
        Ok(self.layout)
    }
    fn core_policy_outcome(
        &self,
        tenant: &TenantId,
        request: &RequestId,
        schema: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<RetainedOutcome>, StateTransactionError> {
        self.checked(|| {
            validate_profile(self, schema, charge)?;
            self.read_outcome_charged(tenant, request, schema, charge)
        })
    }
    fn commit_core_policy(
        self,
        changes: &GroupCorePolicyChanges<'_, '_, '_>,
        charge: &mut PolicySchemaCharge,
    ) -> Result<u16, StateTransactionError> {
        commit::core_policy::apply(self, changes, charge)
    }
}
pub(super) fn validate_profile(
    tx: &RedbStateTransaction<'_>,
    schema: PolicySchemaLimits,
    charge: &PolicySchemaCharge,
) -> Result<(), StateTransactionError> {
    let ceiling = tx.limits.schema();
    if charge.profile() != schema
        || schema.policy_limits().max_states() > ceiling.policy_limits().max_states()
        || schema.max_work_bytes() > ceiling.max_work_bytes()
    {
        return Err(StateTransactionError::InvalidChanges);
    }
    Ok(())
}
