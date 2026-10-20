//! Observable raw due rows; every filtered position and actual end is paid.

use super::super::admission::{storage_error, table_error, value_error};
use super::super::store::{RedbStateTransaction, fence};
use crate::state::key;
use redb::ReadableTable;
use std::ops::Bound;
use wf_domain::claim::apply::ClaimQuery;
use wf_domain::claim::{DueOrder, PartitionFilter};
use wf_ports::state::{ClaimDueKind, ClaimDueStep, GroupClaimDueCursor, StateTransactionError};
use wf_types::identity::TenantId;
use wf_types::version::TimestampMillis;

/// Cursor borrows the existing writer; its one decoded position owns four leaves.
pub struct RedbClaimDueCursor<'tx, 's> {
    transaction: &'tx RedbStateTransaction<'s>,
    tenant: &'tx TenantId,
    query: ClaimQuery<'tx>,
    now: TimestampMillis,
    position: Option<key::DecodedOrderedKey>,
    terminal: Option<bool>,
}

pub(super) fn open<'tx, 's>(
    transaction: &'tx RedbStateTransaction<'s>,
    tenant: &'tx TenantId,
    query: ClaimQuery<'tx>,
    now: TimestampMillis,
) -> Result<RedbClaimDueCursor<'tx, 's>, StateTransactionError> {
    transaction.step()?;
    let scope = transaction.scope(tenant)?;
    let prefix = transaction.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            tenant.as_bytes(),
        ],
        0,
        |cap| key::encode_due_prefix(&scope, cap),
    )?;
    transaction.temporary(prefix.len())?;
    let table = transaction
        .write
        .open_table(transaction.rows_table)
        .map_err(table_error)?;
    let _range = table
        .range(prefix.as_slice()..)
        .map_err(|error| storage_error(&error))?;
    Ok(RedbClaimDueCursor {
        transaction,
        tenant,
        query,
        now,
        position: None,
        terminal: None,
    })
}

impl GroupClaimDueCursor for RedbClaimDueCursor<'_, '_> {
    fn next(&mut self) -> Result<ClaimDueStep<'_>, StateTransactionError> {
        let tx = self.transaction;
        tx.ready()?;
        self.advance().inspect_err(|error| fence(tx.fenced, *error))
    }
    fn position(&self) -> Option<DueOrder<'_>> {
        self.position.as_ref().and_then(key::DecodedOrderedKey::due)
    }
}

impl RedbClaimDueCursor<'_, '_> {
    fn advance(&mut self) -> Result<ClaimDueStep<'_>, StateTransactionError> {
        if let Some(end) = self.terminal {
            return Ok(if end {
                ClaimDueStep::RangeEnd
            } else {
                ClaimDueStep::BudgetEnd
            });
        }
        loop {
            let row = match self.candidate() {
                Err(StateTransactionError::Limit) => {
                    self.terminal = Some(false);
                    return Ok(ClaimDueStep::BudgetEnd);
                }
                row => row?,
            };
            let Some((position, kind)) = row else {
                self.terminal = Some(true);
                return Ok(ClaimDueStep::RangeEnd);
            };
            let due = position.due().ok_or(StateTransactionError::Corrupt)?;
            let matched = due.workflow_type == self.query.workflow_type
                && due.due <= self.now
                && self
                    .query
                    .priority
                    .is_none_or(|priority| priority == due.priority)
                && match self.query.partitions {
                    PartitionFilter::Any => true,
                    PartitionFilter::Only(partitions) => partitions.contains(&due.partition_key),
                };
            self.position = Some(position);
            if matched {
                return self
                    .position()
                    .map(|due| ClaimDueStep::Row { due, kind })
                    .ok_or(StateTransactionError::Corrupt);
            }
        }
    }

    fn candidate(
        &self,
    ) -> Result<Option<(key::DecodedOrderedKey, ClaimDueKind)>, StateTransactionError> {
        let tx = self.transaction;
        tx.step()?;
        let scope = tx.scope(self.tenant)?;
        let prefix = tx.key(
            &[
                scope.group().as_bytes(),
                scope.recovery().as_bytes(),
                self.tenant.as_bytes(),
            ],
            0,
            |cap| key::encode_due_prefix(&scope, cap),
        )?;
        let lower = self
            .position()
            .map(|due| tx.due_key(&scope, due))
            .transpose()?;
        let bound = lower
            .as_ref()
            .map_or(Bound::Included(prefix.as_slice()), |key| {
                Bound::Excluded(key.as_slice())
            });
        tx.temporary(lower.as_ref().map_or(prefix.len(), Vec::len))?;
        let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
        let mut rows = table
            .range::<&[u8]>((bound, Bound::Unbounded))
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
        let kind = match value.value() {
            [0] => ClaimDueKind::Parallel,
            [1] => ClaimDueKind::FifoHead,
            _ => return Err(StateTransactionError::Corrupt),
        };
        tx.temporary(key.value().len())?;
        let owned = key::due_key_owned_bytes(key.value(), &scope, key.value().len())
            .map_err(value_error)?;
        tx.admit_read(owned)?;
        tx.temporary(owned)?;
        key::decode_due_key(key.value(), &scope, key.value().len(), owned)
            .map(|key| Some((key, kind)))
            .map_err(value_error)
    }
}
