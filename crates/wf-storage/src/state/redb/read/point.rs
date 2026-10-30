//! One borrowed point guard, admission, then one independently owned result.

use super::super::{
    admission::{copy_bytes, storage_error, table_error, total, value_error, wire_record_limit},
    store::RedbStateTransaction,
};
use crate::state::{key, value};
use redb::ReadableTable;
use wf_domain::policy::complete::{CompletePolicyValue, PolicySchemaCharge, PolicySchemaLimits};
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::RetainedOutcome;
use wf_ports::state::{StateTransactionError, StoredWorkflowRecord};
use wf_types::identity::{GroupId, PartitionId, RecordId, RequestId, TenantId};

impl RedbStateTransaction<'_> {
    pub(in super::super) fn read_control(&self) -> Result<ControlState, StateTransactionError> {
        self.step()?;
        let key = self.key(
            &[self.group.as_bytes(), self.recovery.as_bytes()],
            0,
            |cap| key::encode_control_key(self.group, self.recovery, cap),
        )?;
        self.opened()?;
        let table = self
            .write
            .open_table(self.rows_table)
            .map_err(table_error)?;
        let guard = table
            .get(key.as_slice())
            .map_err(|e| storage_error(&e))?
            .ok_or(StateTransactionError::Corrupt)?;
        self.recovery_row(&key, guard.value())?;
        self.admit_read(512)?;
        value::decode_control(guard.value(), self.group, self.recovery, 1_024).map_err(value_error)
    }
    pub(in super::super) fn read_nonempty(
        &self,
        tenant: &TenantId,
    ) -> Result<bool, StateTransactionError> {
        self.step()?;
        let scope = self.scope(tenant)?;
        let prefix = self.key(
            &[
                self.group.as_bytes(),
                self.recovery.as_bytes(),
                tenant.as_bytes(),
            ],
            0,
            |cap| key::encode_outcome_prefix(&scope, cap),
        )?;
        self.temporary(prefix.len())?;
        self.opened()?;
        let table = self
            .write
            .open_table(self.rows_table)
            .map_err(table_error)?;
        let mut rows = table
            .range(prefix.as_slice()..)
            .map_err(|e| storage_error(&e))?;
        self.step()?;
        let row = rows.next().transpose().map_err(|e| storage_error(&e))?;
        if let Some((key, value)) = row {
            self.recovery_row(key.value(), value.value())?;
            Ok(key.value().starts_with(&prefix))
        } else {
            Ok(false)
        }
    }
    pub(in super::super) fn read_outcome(
        &self,
        tenant: &TenantId,
        request: &RequestId,
    ) -> Result<Option<RetainedOutcome>, StateTransactionError> {
        let schema = self.limits.schema();
        let mut charge = PolicySchemaCharge::new(schema);
        self.read_outcome_charged(tenant, request, schema, &mut charge)
    }
    pub(in super::super) fn read_outcome_charged(
        &self,
        tenant: &TenantId,
        request: &RequestId,
        schema: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<RetainedOutcome>, StateTransactionError> {
        if charge.profile() != schema {
            return Err(StateTransactionError::InvalidChanges);
        }
        self.step()?;
        let scope = self.scope(tenant)?;
        let key = self.key(
            &[
                self.group.as_bytes(),
                self.recovery.as_bytes(),
                tenant.as_bytes(),
                request.as_bytes(),
            ],
            0,
            |cap| key::encode_outcome_key(&scope, request, cap),
        )?;
        self.opened()?;
        let table = self
            .write
            .open_table(self.rows_table)
            .map_err(table_error)?;
        let guard = table.get(key.as_slice()).map_err(|e| storage_error(&e))?;
        let Some(guard) = guard else {
            return Ok(None);
        };
        self.recovery_row(&key, guard.value())?;
        let maximum = self.outcome_wire_limit(tenant, request)?;
        let validation =
            value::outcome_policy_validation_bytes_in_layout(guard.value(), maximum, self.layout)
                .map_err(value_error)?;
        self.admit_read(validation)?;
        let scanned = value::scan_outcome_in_layout(
            guard.value(),
            &scope,
            request,
            self.admission,
            maximum,
            (schema, charge, validation),
            self.layout,
        )
        .map_err(value_error)?;
        if scanned.intent_version() == 12 && self.value_manifest()?.credit_shard_count.is_none() {
            return Err(StateTransactionError::Corrupt);
        }
        if scanned
            .response_count()
            .is_some_and(|n| n > self.admission.batch_items())
        {
            return Err(StateTransactionError::Limit);
        }
        let bytes = scanned.owned_bytes();
        self.admit_read(bytes)?;
        scanned.into_owned(bytes).map(Some).map_err(value_error)
    }
    pub(in super::super) fn read_placement(
        &self,
        partition: &PartitionId,
    ) -> Result<Option<GroupId>, StateTransactionError> {
        self.step()?;
        let key = self.key(
            &[self.recovery.as_bytes(), partition.as_bytes()],
            0,
            |cap| key::encode_placement_key(self.recovery, partition, cap),
        )?;
        self.opened()?;
        let table = self
            .write
            .open_table(self.rows_table)
            .map_err(table_error)?;
        let guard = table.get(key.as_slice()).map_err(|e| storage_error(&e))?;
        let Some(guard) = guard else {
            return Ok(None);
        };
        self.recovery_row(&key, guard.value())?;
        let bytes = guard.value();
        if bytes.is_empty() || bytes.len() > 256 {
            return Err(StateTransactionError::Corrupt);
        }
        self.admit_read(total(512, [bytes.len()])?)?;
        GroupId::new(copy_bytes(bytes)?)
            .map(Some)
            .map_err(|_| StateTransactionError::Corrupt)
    }
    pub(in super::super) fn read_policy(
        &self,
        tenant: &TenantId,
        workflow_type: &[u8],
        limits: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<CompletePolicyValue>, StateTransactionError> {
        match self.read_stored_policy(tenant, workflow_type, limits, charge)? {
            None => Ok(None),
            Some(wf_ports::state::StoredPolicyValue::Complete(value)) => Ok(Some(*value)),
            Some(wf_ports::state::StoredPolicyValue::Original(_)) => {
                Err(StateTransactionError::UnsupportedPolicyFormat)
            }
        }
    }
    pub(in super::super) fn read_record(
        &self,
        tenant: &TenantId,
        partition: &PartitionId,
        id: &RecordId,
    ) -> Result<Option<StoredWorkflowRecord>, StateTransactionError> {
        self.step()?;
        let scope = self.scope(tenant)?;
        let key = self.record_key(&scope, partition, id)?;
        self.opened()?;
        let table = self
            .write
            .open_table(self.rows_table)
            .map_err(table_error)?;
        let guard = table.get(key.as_slice()).map_err(|e| storage_error(&e))?;
        let Some(guard) = guard else {
            return Ok(None);
        };
        self.recovery_row(&key, guard.value())?;
        let scanned = value::scan_record_in_layout(
            guard.value(),
            &scope,
            partition,
            id,
            self.admission,
            wire_record_limit(self.admission)?,
            self.layout,
        )
        .map_err(value_error)?;
        let bytes = scanned.owned_bytes();
        self.admit_read(bytes)?;
        scanned.into_owned(bytes).map(Some).map_err(value_error)
    }
}
