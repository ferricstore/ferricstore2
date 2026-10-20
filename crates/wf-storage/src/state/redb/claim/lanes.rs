//! Distinct actual lanes, using a paid successor of each complete escaped prefix.

use super::super::admission::{storage_error, table_error, value_error};
use super::super::store::{RedbStateTransaction, fence};
use crate::state::key;
use redb::ReadableTable;
use std::ops::Bound;
use wf_domain::claim::apply::{ClaimQuery, ClaimStates};
use wf_domain::claim::{LaneKey, PartitionFilter};
use wf_ports::state::{ClaimLaneStep, GroupClaimLaneCursor, StateTransactionError};
use wf_types::identity::TenantId;

/// Borrowed writer and one paid discovery-key position; no follower materialization.
pub struct RedbClaimLaneCursor<'tx, 's> {
    transaction: &'tx RedbStateTransaction<'s>,
    tenant: &'tx TenantId,
    query: ClaimQuery<'tx>,
    position: Option<key::DecodedOrderedKey>,
    terminal: Option<bool>,
}

pub(super) fn open<'tx, 's>(
    transaction: &'tx RedbStateTransaction<'s>,
    tenant: &'tx TenantId,
    query: ClaimQuery<'tx>,
) -> Result<RedbClaimLaneCursor<'tx, 's>, StateTransactionError> {
    transaction.step()?;
    let scope = transaction.scope(tenant)?;
    let prefix = transaction.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            tenant.as_bytes(),
            query.workflow_type,
        ],
        0,
        |cap| key::encode_fifo_type_prefix(&scope, query.workflow_type, cap),
    )?;
    transaction.temporary(prefix.len())?;
    let table = transaction
        .write
        .open_table(transaction.rows_table)
        .map_err(table_error)?;
    let _range = table
        .range(prefix.as_slice()..)
        .map_err(|error| storage_error(&error))?;
    Ok(RedbClaimLaneCursor {
        transaction,
        tenant,
        query,
        position: None,
        terminal: None,
    })
}

impl GroupClaimLaneCursor for RedbClaimLaneCursor<'_, '_> {
    fn next(&mut self) -> Result<ClaimLaneStep<'_>, StateTransactionError> {
        let tx = self.transaction;
        tx.ready()?;
        self.advance().inspect_err(|error| fence(tx.fenced, *error))
    }
    fn position(&self) -> Option<LaneKey<'_>> {
        self.position
            .as_ref()
            .and_then(key::DecodedOrderedKey::lane)
    }
}

impl RedbClaimLaneCursor<'_, '_> {
    fn advance(&mut self) -> Result<ClaimLaneStep<'_>, StateTransactionError> {
        if let Some(end) = self.terminal {
            return Ok(if end {
                ClaimLaneStep::RangeEnd
            } else {
                ClaimLaneStep::BudgetEnd
            });
        }
        loop {
            let row = match self.candidate() {
                Err(StateTransactionError::Limit) => {
                    self.terminal = Some(false);
                    return Ok(ClaimLaneStep::BudgetEnd);
                }
                row => row?,
            };
            let Some(position) = row else {
                self.terminal = Some(true);
                return Ok(ClaimLaneStep::RangeEnd);
            };
            let lane = position.lane().ok_or(StateTransactionError::Corrupt)?;
            let matched = self.query.priority.is_none_or(|priority| priority == 0)
                && match self.query.partitions {
                    PartitionFilter::Any => true,
                    PartitionFilter::Only(partitions) => partitions.contains(&lane.partition_key),
                }
                && match self.query.states {
                    ClaimStates::Any => true,
                    ClaimStates::Only(states) => {
                        states.contains(&lane.state) || states.contains(&b"running".as_slice())
                    }
                };
            self.position = Some(position);
            if matched {
                return self
                    .position()
                    .map(ClaimLaneStep::Lane)
                    .ok_or(StateTransactionError::Corrupt);
            }
        }
    }

    fn candidate(&self) -> Result<Option<key::DecodedOrderedKey>, StateTransactionError> {
        let tx = self.transaction;
        tx.step()?;
        let scope = tx.scope(self.tenant)?;
        let prefix = tx.key(
            &[
                scope.group().as_bytes(),
                scope.recovery().as_bytes(),
                self.tenant.as_bytes(),
                self.query.workflow_type,
            ],
            0,
            |cap| key::encode_fifo_type_prefix(&scope, self.query.workflow_type, cap),
        )?;
        let lower = self
            .position()
            .map(|lane| {
                tx.key(
                    &[
                        scope.group().as_bytes(),
                        scope.recovery().as_bytes(),
                        self.tenant.as_bytes(),
                        lane.workflow_type,
                        lane.state,
                        lane.partition_key,
                    ],
                    0,
                    |cap| key::encode_fifo_lane_upper_bound(&scope, lane, cap),
                )
            })
            .transpose()?;
        let bound = lower.as_ref().map_or(prefix.as_slice(), Vec::as_slice);
        tx.temporary(bound.len())?;
        let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
        let mut rows = table
            .range::<&[u8]>((Bound::Included(bound), Bound::Unbounded))
            .map_err(|error| storage_error(&error))?;
        tx.step()?;
        let Some((key, value)) = rows
            .next()
            .transpose()
            .map_err(|error| storage_error(&error))?
        else {
            return Ok(None);
        };
        if !key.value().starts_with(&prefix) {
            return Ok(None);
        }
        if !value.value().is_empty() {
            return Err(StateTransactionError::Corrupt);
        }
        tx.temporary(key.value().len())?;
        let owned = key::fifo_observation_key_owned_bytes(key.value(), &scope, key.value().len())
            .map_err(value_error)?;
        tx.admit_read(owned)?;
        tx.temporary(owned)?;
        key::decode_fifo_observation_key(key.value(), &scope, key.value().len(), owned)
            .map(Some)
            .map_err(value_error)
    }
}
