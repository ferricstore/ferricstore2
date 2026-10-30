//! Explicit consuming history-format upgrade; normal construction stays closed.

use super::{
    RedbStateStore, admission, publication, snapshot, store::RedbStateTransaction, values::manifest,
};
use crate::state::{key, value};
use redb::Durability;
use wf_domain::history::HistoryEntry;
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_domain::workflow::WorkflowRecord;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::StateTransactionError;
use wf_types::identity::PartitionId;

impl RedbStateStore {
    /// Consumes a value-capable owner into the selected complete-history format.
    /// # Errors
    /// Consumes unsupported, corrupt, over-budget or uncertain owners without acknowledgement.
    pub fn upgrade_history_snapshot_layout(self) -> Result<Self, StateTransactionError> {
        self.ready()?;
        if self.layout.get() == 8 {
            return Ok(self);
        }
        if self.layout.get() != 7 {
            return Err(StateTransactionError::UnsupportedLayout);
        }
        upgrade(&self).map_err(|error| self.failed(error))?;
        self.layout.set(8);
        Ok(self)
    }
}

fn upgrade(store: &RedbStateStore) -> Result<(), StateTransactionError> {
    let mut tx = store.transaction(
        false,
        store.limits.admission(),
        store.limits.maximum_recovery_steps(),
        usize::MAX,
        store.limits.maximum_temporary_bytes(),
    )?;
    let mut charge = PolicySchemaCharge::new(store.limits.schema());
    let _ = snapshot::validate::complete(&mut tx, &mut charge, store.limits.snapshot_limits())?;
    let mut selected = manifest::read(&tx)?;
    if selected.layout != 7 {
        return Err(StateTransactionError::Corrupt);
    }
    selected.layout = 8;
    manifest::publish(&tx, selected)?;
    tx.layout = 8;
    let _ = snapshot::validate::complete(&mut tx, &mut charge, store.limits.snapshot_limits())?;
    tx.write
        .set_durability(Durability::Immediate)
        .map_err(|_| StateTransactionError::Unavailable)?;
    tx.write
        .commit()
        .map_err(|_| StateTransactionError::Uncertain)
}

pub(super) fn write(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    partition: &PartitionId,
    record: &WorkflowRecord,
    ordering: OrderingMetadata,
    event: &HistoryEntry,
) -> Result<(), StateTransactionError> {
    let limits = value::HistorySnapshotLimits::new(
        tx.admission,
        tx.limits.maximum_recovery_bytes(),
        1,
        tx.limits.maximum_temporary_bytes(),
    )
    .map_err(admission::value_error)?;
    let size =
        value::history_snapshot_encoded_size(scope, partition, record, ordering, event, limits)
            .map_err(admission::value_error)?;
    let key = tx.output_key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            partition.as_bytes(),
            record.id().as_bytes(),
            event.event_id.as_bytes(),
        ],
        8,
        size,
        |cap| key::encode_history_key(scope, partition, record.id(), event, cap),
    )?;
    admission::engine_pair(key.len(), size)?;
    tx.temporary(admission::total(size, [2048])?)?;
    let encoded =
        value::encode_history_snapshot_fields(scope, partition, record, ordering, event, limits)
            .map_err(admission::value_error)?;
    publication::put(tx, &key, &encoded)
}

pub(super) fn write_steps(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    values: &crate::transaction::ValuePublicationPlan<'_>,
) -> Result<(), StateTransactionError> {
    for step in &values.steps {
        let write_record = step
            .plan
            .plan()
            .write()
            .ok_or(StateTransactionError::InvalidChanges)?;
        let event = step
            .plan
            .plan()
            .history()
            .ok_or(StateTransactionError::InvalidChanges)?;
        write(
            tx,
            scope,
            step.plan.partition(),
            &write_record.record,
            step.ordering,
            event,
        )?;
    }
    Ok(())
}
