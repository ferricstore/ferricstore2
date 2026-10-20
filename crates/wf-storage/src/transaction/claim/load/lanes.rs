//! Actual distinct lanes, finite empty probes and coherent true-head authority.

use super::super::ClaimTransactionError;
use super::{LoadedClaim, copy, debit, entry, reserve_slots, sum};
use wf_domain::claim::apply::{ClaimQuery, ClaimStates};
use wf_domain::claim::{DueOrder, LaneKey, LaneOrder, PartitionFilter};
use wf_domain::policy::definition::StateMode;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{
    ClaimDueKind, ClaimLaneStep, GroupClaimLaneCursor, GroupClaimTransaction,
    StateTransactionError, StoredWorkflowRecord,
};
use wf_types::identity::PartitionId;

impl LoadedClaim {
    pub(super) fn load_lanes<T: GroupClaimTransaction>(
        &mut self,
        tx: &T,
        scope: &RequestScope,
        query: ClaimQuery<'_>,
        budget: usize,
        cap: usize,
    ) -> Result<(), ClaimTransactionError> {
        let mut cursor = tx.claim_lane_cursor(scope.tenant(), query)?;
        loop {
            match cursor.next()? {
                ClaimLaneStep::RangeEnd => break,
                ClaimLaneStep::BudgetEnd => return Err(StateTransactionError::Limit.into()),
                ClaimLaneStep::Lane(lane) => {
                    if self.mode(lane.state) != StateMode::Fifo {
                        return Err(StateTransactionError::Corrupt.into());
                    }
                    if !self.load_head(tx, scope, lane, budget, cap)? {
                        return Ok(());
                    }
                }
            }
        }
        drop(cursor);
        if query.priority.is_none_or(|priority| priority == 0)
            && let (PartitionFilter::Only(partitions), ClaimStates::Only(states)) =
                (query.partitions, query.states)
        {
            for state in states {
                if *state == b"running" || self.mode(state) != StateMode::Fifo {
                    continue;
                }
                for partition in partitions {
                    let lane = LaneKey {
                        workflow_type: query.workflow_type,
                        state,
                        partition_key: partition,
                    };
                    if !self.load_head(tx, scope, lane, budget, cap)? {
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }

    fn load_head<T: GroupClaimTransaction>(
        &mut self,
        tx: &T,
        scope: &RequestScope,
        lane: LaneKey<'_>,
        budget: usize,
        cap: usize,
    ) -> Result<bool, ClaimTransactionError> {
        if self.heads.iter().any(|(state, partition, _)| {
            state == lane.state && partition.as_bytes() == lane.partition_key
        }) {
            return Ok(true);
        }
        if self.heads.len() == budget {
            self.heads_complete = false;
            return Ok(false);
        }
        reserve_slots(&mut self.heads, 1, &mut self.staging_used, cap)?;
        debit(
            &mut self.staging_used,
            sum(&[lane.state.len(), lane.partition_key.len()])?,
            cap,
        )?;
        let state = copy(lane.state)?;
        let partition = PartitionId::new(copy(lane.partition_key)?)
            .map_err(|_| StateTransactionError::Corrupt)?;
        self.load_placement(tx, scope, &partition, cap)?;
        let head = tx.fifo_head(scope.tenant(), lane)?;
        let index = if let Some(head) = head {
            if head.partition != partition
                || head.tenant != *scope.tenant()
                || LaneKey::for_record(&entry(&head)) != Some(lane)
            {
                return Err(StateTransactionError::Corrupt.into());
            }
            self.verify_fifo(tx, scope, &head)?;
            Some(self.store_record(head, cap)?)
        } else {
            None
        };
        self.heads.push((state, partition, index));
        Ok(true)
    }

    pub(super) fn verify_fifo<T: GroupClaimTransaction>(
        &self,
        tx: &T,
        scope: &RequestScope,
        owner: &StoredWorkflowRecord,
    ) -> Result<(), ClaimTransactionError> {
        let row = entry(owner);
        let lane = LaneKey::for_record(&row).ok_or(StateTransactionError::Corrupt)?;
        if row.priority != 0
            || self.mode(row.logical_state()) != StateMode::Fifo
            || tx
                .fifo_member(scope.tenant(), lane, LaneOrder::for_record(&row))?
                .as_ref()
                != Some(owner)
            || tx.fifo_head(scope.tenant(), lane)?.as_ref() != Some(owner)
        {
            return Err(StateTransactionError::Corrupt.into());
        }
        if let Some(due) = DueOrder::for_record(&row)
            && tx.claim_due_kind(scope.tenant(), due)? != Some(ClaimDueKind::FifoHead)
        {
            return Err(StateTransactionError::Corrupt.into());
        }
        Ok(())
    }
}
