//! Coherent local facts; no distributed release-authorizing freshness is implied.
use super::{
    super::{
        admission::{copy_bytes, total},
        store::RedbStateStore,
    },
    read,
};
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{
    GovernanceCatalogReadLimits as Limits, GroupApplyTransaction, StateTransactionError as E,
    WorkflowReleasePage, WorkflowReleaseReadPort, WorkflowReleaseRecord as Record,
};
use wf_types::identity::{PartitionId, RecordId};

impl WorkflowReleaseReadPort for RedbStateStore {
    fn workflow_release_page(
        &mut self,
        scope: &RequestScope,
        start: Option<u64>,
        limit: u16,
        limits: Limits,
    ) -> Result<WorkflowReleasePage, E> {
        self.ready()?;
        let tx = self.transaction(
            false,
            self.limits.admission(),
            limits.maximum_steps,
            limits.maximum_bytes,
            limits.maximum_temporary_bytes,
        )?;
        tx.checked(|| read::reconcile_page(&tx, scope, start, limit))
    }
    // The release-authorizing record read deliberately keeps the trait's closed default.
}
impl RedbStateStore {
    /// Loads bounded physical state and binding facts under one local guard.
    /// A configured production caller still needs accepted current read authority.
    /// # Errors
    /// Refuses unsupported local routing, malformed data or cumulative admission.
    pub fn read_workflow_release_records(
        &self,
        scope: &RequestScope,
        partition: Option<&[u8]>,
        ids: &[&[u8]],
        limits: Limits,
    ) -> Result<Vec<Record>, E> {
        if ids.is_empty() || ids.len() > 256 {
            return Err(E::InvalidChanges);
        }
        let partition = partition
            .filter(|p| !p.is_empty())
            .ok_or(E::UnsupportedPolicyProfile)?;
        self.ready()?;
        let tx = self.transaction(
            false,
            self.limits.admission(),
            limits.maximum_steps,
            limits.maximum_bytes,
            limits.maximum_temporary_bytes,
        )?;
        tx.checked(|| records(&tx, scope, partition, ids))
    }
}

fn records(
    tx: &super::super::store::RedbStateTransaction<'_>,
    scope: &RequestScope,
    partition: &[u8],
    ids: &[&[u8]],
) -> Result<Vec<Record>, E> {
    tx.require_workflow_release()?;
    if scope.group() != tx.group || scope.recovery() != tx.recovery {
        return Err(E::NamespaceConflict);
    }
    tx.admit_read(total(
        512,
        [
            partition.len(),
            ids.len().checked_mul(size_of::<Record>()).ok_or(E::Limit)?,
        ],
    )?)?;
    let partition = PartitionId::new(copy_bytes(partition)?).map_err(|_| E::InvalidChanges)?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(ids.len())
        .map_err(|_| E::Allocation)?;
    for id in ids {
        tx.admit_read(total(128, [id.len()])?)?;
        let id = RecordId::new(copy_bytes(id)?).map_err(|_| E::InvalidChanges)?;
        let Some(stored) = tx.record(scope.tenant(), &partition, &id)? else {
            result.push(Record::Missing);
            continue;
        };
        let binding = tx.workflow_governance_binding(scope, &partition, &id)?;
        let state = stored.record.state();
        let reservation = binding.as_ref().map(|(_, r)| r.reservation_id.as_slice());
        tx.admit_read(total(
            128,
            [state.len(), reservation.map_or(0, <[u8]>::len)],
        )?)?;
        result.push(Record::Record {
            state: Some(copy_bytes(state)?),
            reservation_id: reservation.map(copy_bytes).transpose()?,
        });
    }
    Ok(result)
}
