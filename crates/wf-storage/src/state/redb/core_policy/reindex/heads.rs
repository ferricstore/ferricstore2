//! Owned original head ID or actual empty lane, never recaptured after mutation.

use super::super::super::{
    admission::{copy_bytes, entry, storage_error, table_error, total},
    publication,
    store::RedbStateTransaction,
};
use super::mode;
use redb::ReadableTable;
use std::collections::BTreeMap;
use wf_domain::claim::{DueOrder, LaneKey};
use wf_domain::policy::complete::CompletePolicyValue;
use wf_domain::policy::definition::{PolicySnapshot, StateMode};
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{GroupApplyTransaction, StateTransactionError, StoredWorkflowRecord};
use wf_types::identity::{PartitionId, RecordId};

pub(in crate::state::redb) struct PolicyReindexHeads {
    lanes: BTreeMap<PartitionId, BTreeMap<Vec<u8>, Option<RecordId>>>,
}
impl PolicyReindexHeads {
    pub(super) fn new(tx: &RedbStateTransaction<'_>) -> Result<Self, StateTransactionError> {
        tx.temporary(512)?;
        Ok(Self {
            lanes: BTreeMap::new(),
        })
    }
    pub(super) fn capture(
        &mut self,
        tx: &RedbStateTransaction<'_>,
        scope: &RequestScope,
        owner: &StoredWorkflowRecord,
        old: StateMode,
    ) -> Result<(), StateTransactionError> {
        let record = entry(owner);
        let lane = LaneKey::for_record(&record).ok_or(StateTransactionError::Corrupt)?;
        if self
            .lanes
            .get(&owner.partition)
            .is_some_and(|m| m.contains_key(lane.state))
        {
            return Ok(());
        }
        let head = tx.fifo_head(scope.tenant(), lane)?;
        if (old == StateMode::Fifo) != head.is_some() {
            return Err(StateTransactionError::Corrupt);
        }
        if let Some(head) = &head {
            if head.ordering.priority > 2 || LaneKey::for_record(&entry(head)) != Some(lane) {
                return Err(StateTransactionError::Corrupt);
            }
            if let Some(due) = DueOrder::for_record(&entry(head)) {
                let key = tx.due_key(scope, due)?;
                tx.step()?;
                let mut table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
                tx.step()?;
                {
                    let value = table
                        .get(key.as_slice())
                        .map_err(|e| storage_error(&e))?
                        .ok_or(StateTransactionError::Corrupt)?;
                    if value.value() != [1] {
                        return Err(StateTransactionError::Corrupt);
                    }
                }
                self.retain(tx, owner, lane.state, Some(head.record.id()))?;
                tx.step()?;
                let removed = table
                    .remove(key.as_slice())
                    .map_err(|e| storage_error(&e))?
                    .ok_or(StateTransactionError::Corrupt)?;
                if removed.value() != [1] {
                    return Err(StateTransactionError::Corrupt);
                }
                return Ok(());
            }
        }
        self.retain(tx, owner, lane.state, head.as_ref().map(|h| h.record.id()))
    }
    fn retain(
        &mut self,
        tx: &RedbStateTransaction<'_>,
        owner: &StoredWorkflowRecord,
        state: &[u8],
        head: Option<&RecordId>,
    ) -> Result<(), StateTransactionError> {
        if !self.lanes.contains_key(&owner.partition) {
            tx.temporary(total(512, [owner.partition.as_bytes().len()])?)?;
            let partition = PartitionId::new(copy_bytes(owner.partition.as_bytes())?)
                .map_err(|_| StateTransactionError::Corrupt)?;
            self.lanes.insert(partition, BTreeMap::new());
        }
        tx.temporary(total(
            512,
            [state.len(), head.map_or(0, |id| id.as_bytes().len())],
        )?)?;
        let copied = head
            .map(|id| {
                RecordId::new(copy_bytes(id.as_bytes())?)
                    .map_err(|_| StateTransactionError::Corrupt)
            })
            .transpose()?;
        self.lanes
            .get_mut(&owner.partition)
            .ok_or(StateTransactionError::Corrupt)?
            .insert(copy_bytes(state)?, copied);
        Ok(())
    }
    pub(in crate::state::redb) fn finish(
        &self,
        tx: &RedbStateTransaction<'_>,
        scope: &RequestScope,
        policy: &CompletePolicyValue,
    ) -> Result<(), StateTransactionError> {
        self.finish_for(tx, scope, &policy.data().workflow_type, |state| {
            mode(Some(policy), state)
        })
    }
    pub(in crate::state::redb) fn finish_modes(
        &self,
        tx: &RedbStateTransaction<'_>,
        scope: &RequestScope,
        policy: &PolicySnapshot,
    ) -> Result<(), StateTransactionError> {
        self.finish_for(tx, scope, policy.workflow_type(), |state| {
            policy.state_mode(state)
        })
    }
    fn finish_for(
        &self,
        tx: &RedbStateTransaction<'_>,
        scope: &RequestScope,
        kind: &[u8],
        mode: impl Fn(&[u8]) -> StateMode,
    ) -> Result<(), StateTransactionError> {
        for (partition, states) in &self.lanes {
            for state in states.keys() {
                let lane = LaneKey {
                    workflow_type: kind,
                    state,
                    partition_key: partition.as_bytes(),
                };
                let head = tx.fifo_head(scope.tenant(), lane)?;
                if mode(state) == StateMode::Parallel {
                    if head.is_some() {
                        return Err(StateTransactionError::Corrupt);
                    }
                } else if let Some(head) = head
                    && let Some(due) = DueOrder::for_record(&entry(&head))
                {
                    let key = tx.due_key(scope, due)?;
                    publication::put(tx, &key, &[1])?;
                }
            }
        }
        Ok(())
    }
}
