//! Restricted physical Claim extensions on the existing exclusive writer guard.

use super::admission::{storage_error, table_error};
use super::store::RedbStateTransaction;
use redb::ReadableTable;
use wf_domain::claim::DueOrder;
use wf_domain::claim::apply::ClaimQuery;
use wf_ports::state::{
    ClaimDueKind, GroupClaimChanges, GroupClaimTransaction, StateTransactionError,
};
use wf_types::identity::TenantId;
use wf_types::version::TimestampMillis;

mod due;
mod lanes;

impl<'s> GroupClaimTransaction for RedbStateTransaction<'s> {
    type ClaimDueCursor<'tx>
        = due::RedbClaimDueCursor<'tx, 's>
    where
        Self: 'tx;
    type ClaimLaneCursor<'tx>
        = lanes::RedbClaimLaneCursor<'tx, 's>
    where
        Self: 'tx;

    fn claim_due_cursor<'tx>(
        &'tx self,
        tenant: &'tx TenantId,
        query: ClaimQuery<'tx>,
        now: TimestampMillis,
    ) -> Result<Self::ClaimDueCursor<'tx>, StateTransactionError> {
        self.checked(|| due::open(self, tenant, query, now))
    }
    fn claim_lane_cursor<'tx>(
        &'tx self,
        tenant: &'tx TenantId,
        query: ClaimQuery<'tx>,
    ) -> Result<Self::ClaimLaneCursor<'tx>, StateTransactionError> {
        self.checked(|| lanes::open(self, tenant, query))
    }
    fn claim_due_kind(
        &self,
        tenant: &TenantId,
        due: DueOrder<'_>,
    ) -> Result<Option<ClaimDueKind>, StateTransactionError> {
        self.checked(|| {
            self.step()?;
            let scope = self.scope(tenant)?;
            let key = self.due_key(&scope, due)?;
            let table = self
                .write
                .open_table(self.rows_table)
                .map_err(table_error)?;
            let guard = table
                .get(key.as_slice())
                .map_err(|error| storage_error(&error))?;
            match guard.as_ref().map(redb::AccessGuard::value) {
                None => Ok(None),
                Some([0]) => Ok(Some(ClaimDueKind::Parallel)),
                Some([1]) => Ok(Some(ClaimDueKind::FifoHead)),
                _ => Err(StateTransactionError::Corrupt),
            }
        })
    }
    fn commit_claim(
        self,
        changes: &GroupClaimChanges<'_, '_>,
    ) -> Result<(), StateTransactionError> {
        super::commit::claim::apply(self, changes)
    }
}
