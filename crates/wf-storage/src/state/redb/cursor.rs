//! Borrowed same-writer reseeks and fully admitted continuation positions.

use super::{
    admission::{entry, storage_error, table_error, total, value_error},
    store::{RedbStateTransaction, fence},
};
use crate::state::key;
use redb::ReadableTable;
use std::ops::Bound;
use wf_domain::claim::apply::{ClaimQuery, ClaimStates};
use wf_domain::claim::{DueOrder, LaneKey, PartitionFilter};
use wf_ports::state::{
    DueCursorStep, GroupApplyTransaction, GroupDueCursor, StateTransactionError,
};
use wf_types::identity::TenantId;

/// Cursor holding no self-referential engine guard or range.
pub struct RedbStateDueCursor<'tx, 's> {
    transaction: &'tx RedbStateTransaction<'s>,
    tenant: &'tx TenantId,
    query: ClaimQuery<'tx>,
    after: Option<DueOrder<'tx>>,
    position: Option<key::DecodedOrderedKey>,
    maximum_rows: usize,
    maximum_bytes: usize,
    returned: usize,
    bytes: usize,
    terminal: Option<bool>,
}

pub(super) fn open<'tx, 's>(
    transaction: &'tx RedbStateTransaction<'s>,
    tenant: &'tx TenantId,
    query: ClaimQuery<'tx>,
    after: Option<DueOrder<'tx>>,
    maximum_rows: usize,
    maximum_bytes: usize,
) -> Result<RedbStateDueCursor<'tx, 's>, StateTransactionError> {
    transaction.step()?;
    if maximum_rows == 0 || maximum_bytes == 0 {
        return Err(StateTransactionError::Limit);
    }
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
    {
        let table = transaction
            .write
            .open_table(transaction.rows_table)
            .map_err(table_error)?;
        let _range = table
            .range(prefix.as_slice()..)
            .map_err(|e| storage_error(&e))?;
    }
    Ok(RedbStateDueCursor {
        transaction,
        tenant,
        query,
        after,
        position: None,
        maximum_rows,
        maximum_bytes,
        returned: 0,
        bytes: 0,
        terminal: None,
    })
}

impl GroupDueCursor for RedbStateDueCursor<'_, '_> {
    fn next(&mut self) -> Result<DueCursorStep<'_>, StateTransactionError> {
        let tx = self.transaction;
        tx.ready()?;
        self.advance().inspect_err(|error| fence(tx.fenced, *error))
    }
    fn position(&self) -> Option<DueOrder<'_>> {
        self.position.as_ref().and_then(key::DecodedOrderedKey::due)
    }
}

impl RedbStateDueCursor<'_, '_> {
    fn advance(&mut self) -> Result<DueCursorStep<'_>, StateTransactionError> {
        if let Some(end) = self.terminal {
            return Ok(if end {
                DueCursorStep::RangeEnd
            } else {
                DueCursorStep::BudgetEnd
            });
        }
        loop {
            if self.returned == self.maximum_rows {
                return Ok(self.budget_end());
            }
            let next = match self.candidate() {
                Err(StateTransactionError::Limit) => return Ok(self.budget_end()),
                other => other?,
            };
            let Some((decoded, tag)) = next else {
                self.terminal = Some(true);
                return Ok(DueCursorStep::RangeEnd);
            };
            let bytes = decoded.logical_bytes();
            let Some(next_bytes) = self
                .bytes
                .checked_add(bytes)
                .filter(|n| *n <= self.maximum_bytes)
            else {
                return Ok(self.budget_end());
            };
            match self.transaction.admit_read(bytes) {
                Ok(()) => {}
                Err(StateTransactionError::Limit) => return Ok(self.budget_end()),
                Err(error) => return Err(error),
            }
            let matched = match self.matches(&decoded, tag) {
                Err(StateTransactionError::Limit) => return Ok(self.budget_end()),
                other => other?,
            };
            self.position = Some(decoded);
            self.after = None;
            self.bytes = next_bytes;
            if matched {
                self.returned += 1;
                return self
                    .position()
                    .map(DueCursorStep::Row)
                    .ok_or(StateTransactionError::Corrupt);
            }
        }
    }
    fn budget_end(&mut self) -> DueCursorStep<'_> {
        self.terminal = Some(false);
        DueCursorStep::BudgetEnd
    }
    fn candidate(&self) -> Result<Option<(key::DecodedOrderedKey, u8)>, StateTransactionError> {
        self.transaction.step()?;
        let scope = self.transaction.scope(self.tenant)?;
        let prefix = self.transaction.key(
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
            .or(self.after)
            .map(|due| self.transaction.due_key(&scope, due))
            .transpose()?;
        let bound = lower
            .as_ref()
            .map_or(Bound::Included(prefix.as_slice()), |key| {
                Bound::Excluded(key.as_slice())
            });
        self.transaction
            .temporary(lower.as_ref().map_or(prefix.len(), Vec::len))?;
        let table = self
            .transaction
            .write
            .open_table(self.transaction.rows_table)
            .map_err(table_error)?;
        let mut rows = table
            .range::<&[u8]>((bound, Bound::Unbounded))
            .map_err(|e| storage_error(&e))?;
        self.transaction.step()?;
        let Some((key, value)) = rows.next().transpose().map_err(|e| storage_error(&e))? else {
            return Ok(None);
        };
        if !key.value().starts_with(&prefix) {
            return Ok(None);
        }
        let tag = match value.value() {
            [tag @ (0 | 1)] => *tag,
            _ => return Err(StateTransactionError::Corrupt),
        };
        let maximum = total(128, [key.value().len()])?;
        self.transaction.temporary(maximum)?;
        key::decode_due_key(key.value(), &scope, key.value().len(), maximum)
            .map(|row| Some((row, tag)))
            .map_err(value_error)
    }
    fn matches(
        &self,
        decoded: &key::DecodedOrderedKey,
        tag: u8,
    ) -> Result<bool, StateTransactionError> {
        let due = decoded.due().ok_or(StateTransactionError::Corrupt)?;
        let owner = self
            .transaction
            .point_from_bytes(self.tenant, due.partition_key, due.id)?
            .ok_or(StateTransactionError::Corrupt)?;
        let actual = entry(&owner);
        if DueOrder::for_record(&actual) != Some(due) {
            return Err(StateTransactionError::Corrupt);
        }
        if tag == 1 {
            let lane = LaneKey::for_record(&actual).ok_or(StateTransactionError::Corrupt)?;
            let head = self
                .transaction
                .fifo_head(self.tenant, lane)?
                .ok_or(StateTransactionError::Corrupt)?;
            if head != owner {
                return Err(StateTransactionError::Corrupt);
            }
        }
        Ok(due.workflow_type == self.query.workflow_type
            && self.query.priority.is_none_or(|p| p == due.priority)
            && match self.query.partitions {
                PartitionFilter::Any => true,
                PartitionFilter::Only(v) => v.contains(&due.partition_key),
            }
            && match self.query.states {
                ClaimStates::Any => true,
                ClaimStates::Only(v) => v.contains(&owner.record.state()),
            }
            && !self.query.exclude_states.contains(&owner.record.state()))
    }
}
