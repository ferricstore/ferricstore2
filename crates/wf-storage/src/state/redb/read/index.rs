//! Real physical membership and the first FIFO row in the same writer revision.

use super::super::{
    admission::{copy_bytes, entry, storage_error, table_error, total, value_error},
    store::RedbStateTransaction,
};
use crate::state::key;
use redb::ReadableTable;
use wf_domain::claim::{DueOrder, LaneKey, LaneOrder};
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{GroupApplyTransaction, StateTransactionError, StoredWorkflowRecord};
use wf_types::identity::{PartitionId, RecordId, TenantId};

impl RedbStateTransaction<'_> {
    pub(in super::super) fn read_parallel(
        &self,
        tenant: &TenantId,
        due: DueOrder<'_>,
    ) -> Result<bool, StateTransactionError> {
        self.step()?;
        let scope = self.scope(tenant)?;
        let key = self.due_key(&scope, due)?;
        self.opened()?;
        let table = self
            .write
            .open_table(self.rows_table)
            .map_err(table_error)?;
        let guard = table.get(key.as_slice()).map_err(|e| storage_error(&e))?;
        if let Some(row) = &guard {
            self.recovery_row(&key, row.value())?;
        }
        match guard.as_ref().map(redb::AccessGuard::value) {
            None | Some([1]) => Ok(false),
            Some([0]) => Ok(true),
            _ => Err(StateTransactionError::Corrupt),
        }
    }
    pub(in super::super) fn read_member(
        &self,
        tenant: &TenantId,
        lane: LaneKey<'_>,
        member: LaneOrder<'_>,
    ) -> Result<Option<StoredWorkflowRecord>, StateTransactionError> {
        self.step()?;
        let scope = self.scope(tenant)?;
        let key = self.fifo_key(&scope, lane, member)?;
        self.opened()?;
        {
            let table = self
                .write
                .open_table(self.rows_table)
                .map_err(table_error)?;
            let guard = table.get(key.as_slice()).map_err(|e| storage_error(&e))?;
            let Some(guard) = guard else {
                return Ok(None);
            };
            self.recovery_row(&key, guard.value())?;
            if !guard.value().is_empty() {
                return Err(StateTransactionError::Corrupt);
            }
        }
        let owner = self
            .point_from_bytes(tenant, lane.partition_key, member.id())?
            .ok_or(StateTransactionError::Corrupt)?;
        let actual = entry(&owner);
        if LaneKey::for_record(&actual) != Some(lane) || LaneOrder::for_record(&actual) != member {
            return Err(StateTransactionError::Corrupt);
        }
        Ok(Some(owner))
    }
    pub(in super::super) fn read_head(
        &self,
        tenant: &TenantId,
        lane: LaneKey<'_>,
    ) -> Result<Option<StoredWorkflowRecord>, StateTransactionError> {
        self.step()?;
        let scope = self.scope(tenant)?;
        let prefix = self.key(
            &[
                self.group.as_bytes(),
                self.recovery.as_bytes(),
                tenant.as_bytes(),
                lane.workflow_type,
                lane.state,
                lane.partition_key,
            ],
            0,
            |cap| key::encode_fifo_prefix(&scope, lane, cap),
        )?;
        let Some(decoded) = self.first_member(&scope, lane, &prefix)? else {
            return Ok(None);
        };
        let (member_score, sequence, id) =
            decoded.member().ok_or(StateTransactionError::Corrupt)?;
        let owner = self
            .point_from_bytes(tenant, lane.partition_key, id)?
            .ok_or(StateTransactionError::Corrupt)?;
        let actual = entry(&owner);
        let member = LaneOrder::for_record(&actual);
        if decoded.lane() != Some(lane)
            || LaneKey::for_record(&actual) != Some(lane)
            || (member.score(), member.sequence(), member.id()) != (member_score, sequence, id)
        {
            return Err(StateTransactionError::Corrupt);
        }
        Ok(Some(owner))
    }
    pub(in super::super) fn point_from_bytes(
        &self,
        tenant: &TenantId,
        partition: &[u8],
        id: &[u8],
    ) -> Result<Option<StoredWorkflowRecord>, StateTransactionError> {
        self.temporary(total(partition.len(), [id.len()])?)?;
        let partition =
            PartitionId::new(copy_bytes(partition)?).map_err(|_| StateTransactionError::Corrupt)?;
        let id = RecordId::new(copy_bytes(id)?).map_err(|_| StateTransactionError::Corrupt)?;
        self.record(tenant, &partition, &id)
    }
    fn first_member(
        &self,
        scope: &RequestScope,
        lane: LaneKey<'_>,
        prefix: &[u8],
    ) -> Result<Option<key::DecodedOrderedKey>, StateTransactionError> {
        self.temporary(prefix.len())?;
        self.opened()?;
        let table = self
            .write
            .open_table(self.rows_table)
            .map_err(table_error)?;
        let mut rows = table.range(prefix..).map_err(|e| storage_error(&e))?;
        self.opened()?;
        self.step()?;
        let Some((key, value)) = rows.next().transpose().map_err(|e| storage_error(&e))? else {
            return Ok(None);
        };
        self.recovery_row(key.value(), value.value())?;
        if !key.value().starts_with(prefix) {
            return Ok(None);
        }
        if !value.value().is_empty() {
            return Err(StateTransactionError::Corrupt);
        }
        let maximum = total(128, [key.value().len()])?;
        self.temporary(maximum)?;
        key::decode_fifo_key(key.value(), scope, lane, key.value().len(), maximum)
            .map(Some)
            .map_err(value_error)
    }
}
