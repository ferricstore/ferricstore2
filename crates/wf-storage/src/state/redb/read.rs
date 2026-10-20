//! Existing semantic ports, with fatal errors latched on the same live guard.

use super::{commit, cursor, store::RedbStateTransaction};
use wf_domain::claim::apply::ClaimQuery;
use wf_domain::claim::{DueOrder, LaneKey, LaneOrder};
use wf_domain::policy::complete::{CompletePolicyValue, PolicySchemaCharge, PolicySchemaLimits};
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::RetainedOutcome;
use wf_ports::state::{
    GroupApplyChanges, GroupApplyTransaction, StateTransactionError, StoredPolicyValue,
    StoredWorkflowRecord,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RequestId, TenantId};

mod index;
mod point;

impl<'s> GroupApplyTransaction for RedbStateTransaction<'s> {
    type DueCursor<'tx>
        = cursor::RedbStateDueCursor<'tx, 's>
    where
        Self: 'tx;
    fn control(&self) -> Result<ControlState, StateTransactionError> {
        self.checked(|| self.read_control())
    }
    fn retained_nonempty(&self, tenant: &TenantId) -> Result<bool, StateTransactionError> {
        self.checked(|| self.read_nonempty(tenant))
    }
    fn outcome(
        &self,
        tenant: &TenantId,
        request: &RequestId,
    ) -> Result<Option<RetainedOutcome>, StateTransactionError> {
        self.checked(|| self.read_outcome(tenant, request))
    }
    fn placement(&self, partition: &PartitionId) -> Result<Option<GroupId>, StateTransactionError> {
        self.checked(|| self.read_placement(partition))
    }
    fn installed_policy(
        &self,
        tenant: &TenantId,
        workflow_type: &[u8],
        limits: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<CompletePolicyValue>, StateTransactionError> {
        self.checked(|| self.read_policy(tenant, workflow_type, limits, charge))
    }
    fn stored_policy_value(
        &self,
        tenant: &TenantId,
        kind: &[u8],
        schema: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<StoredPolicyValue>, StateTransactionError> {
        self.checked(|| self.read_stored_policy(tenant, kind, schema, charge))
    }
    fn record(
        &self,
        tenant: &TenantId,
        partition: &PartitionId,
        id: &RecordId,
    ) -> Result<Option<StoredWorkflowRecord>, StateTransactionError> {
        self.checked(|| self.read_record(tenant, partition, id))
    }
    fn parallel_member(
        &self,
        tenant: &TenantId,
        due: DueOrder<'_>,
    ) -> Result<bool, StateTransactionError> {
        self.checked(|| self.read_parallel(tenant, due))
    }
    fn fifo_member(
        &self,
        tenant: &TenantId,
        lane: LaneKey<'_>,
        member: LaneOrder<'_>,
    ) -> Result<Option<StoredWorkflowRecord>, StateTransactionError> {
        self.checked(|| self.read_member(tenant, lane, member))
    }
    fn fifo_head(
        &self,
        tenant: &TenantId,
        lane: LaneKey<'_>,
    ) -> Result<Option<StoredWorkflowRecord>, StateTransactionError> {
        self.checked(|| self.read_head(tenant, lane))
    }
    fn due_cursor<'tx>(
        &'tx self,
        tenant: &'tx TenantId,
        query: ClaimQuery<'tx>,
        after: Option<DueOrder<'tx>>,
        maximum_rows: usize,
        maximum_bytes: usize,
    ) -> Result<Self::DueCursor<'tx>, StateTransactionError> {
        self.checked(|| cursor::open(self, tenant, query, after, maximum_rows, maximum_bytes))
    }
    fn commit(self, changes: &GroupApplyChanges<'_, '_>) -> Result<(), StateTransactionError> {
        commit::apply(self, changes)
    }
}
