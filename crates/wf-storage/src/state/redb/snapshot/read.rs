//! Returned metadata is admitted once in R before any typed owner is copied.

use super::super::{
    membership,
    store::{RedbStateStore, RedbStateTransaction},
};
use super::{SnapshotError, SnapshotLimits, SnapshotMetadata, admission, format, select, validate};
use crate::state::value;
use redb::ReadableTable;
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_ports::state::{GroupApplyTransaction, StateTransactionError as Error};
use wf_types::identity::{GroupId, RecoveryId};

pub(super) fn metadata(
    tx: &RedbStateTransaction<'_>,
    facts: ([u8; 32], usize, usize),
) -> Result<SnapshotMetadata, SnapshotError> {
    let control = tx.control().map_err(SnapshotError::State)?;
    tx.step().map_err(SnapshotError::State)?;
    let table = tx
        .write
        .open_table(tx.metadata_table)
        .map_err(admission::table_error)
        .map_err(SnapshotError::State)?;
    tx.step().map_err(SnapshotError::State)?;
    let bytes = table
        .get(membership::KEY)
        .map_err(|e| SnapshotError::State(admission::storage_error(&e)))?
        .ok_or(SnapshotError::State(Error::Corrupt))?;
    tx.recovery_row(membership::KEY, bytes.value())
        .map_err(SnapshotError::State)?;
    let profile = tx.limits.membership_limits();
    let owner = value::measure_applied_membership(bytes.value(), tx.group, tx.recovery, profile)
        .map_err(admission::value_error)
        .map_err(SnapshotError::State)?;
    let required = admission::total(
        1024,
        [
            tx.group.as_bytes().len(),
            tx.recovery.as_bytes().len(),
            owner,
        ],
    )
    .map_err(|_| SnapshotError::Limit)?;
    admission::debit(&tx.read, required, tx.maximum_read).map_err(|_| SnapshotError::Limit)?;
    let group = GroupId::new(
        admission::copy_bytes(tx.group.as_bytes()).map_err(|_| SnapshotError::Allocation)?,
    )
    .map_err(|_| SnapshotError::Malformed)?;
    let recovery = RecoveryId::new(
        admission::copy_bytes(tx.recovery.as_bytes()).map_err(|_| SnapshotError::Allocation)?,
    )
    .map_err(|_| SnapshotError::Malformed)?;
    let membership =
        value::decode_applied_membership(bytes.value(), tx.group, tx.recovery, profile, owner)
            .map_err(admission::value_error)
            .map_err(SnapshotError::State)?;
    SnapshotMetadata::new(
        (group, recovery),
        control,
        membership,
        facts.0,
        (facts.1, facts.2),
    )
}

pub(super) fn current<W: std::io::Write>(
    store: &RedbStateStore,
    output: &mut W,
    limits: SnapshotLimits,
) -> Result<Option<SnapshotMetadata>, SnapshotError> {
    let mut tx = super::transaction(store, limits, store.limits).map_err(SnapshotError::State)?;
    let mut charge = PolicySchemaCharge::new(store.limits.schema());
    let Some(facts) =
        validate::complete(&mut tx, &mut charge, limits).map_err(SnapshotError::State)?
    else {
        return Ok(None);
    };
    if matches!(store.layout.get(), 4..=7) {
        let catalog = super::catalog::load(&tx).map_err(SnapshotError::State)?;
        super::catalog::select(
            &mut tx,
            catalog
                .current
                .ok_or(SnapshotError::State(Error::Corrupt))?,
        );
    } else {
        select(&mut tx, true);
    }
    let metadata = metadata(&tx, facts)?;
    if format::emit(&tx, Some(output), limits)? != facts {
        return Err(SnapshotError::State(Error::Corrupt));
    }
    Ok(Some(metadata))
}

pub(super) fn pin(
    store: &RedbStateStore,
    expected: Option<[u8; 32]>,
    limits: SnapshotLimits,
) -> Result<Option<[u8; 32]>, SnapshotError> {
    let mut tx = super::transaction(store, limits, store.limits).map_err(SnapshotError::State)?;
    let mut charge = PolicySchemaCharge::new(store.limits.schema());
    let facts = validate::complete(&mut tx, &mut charge, limits).map_err(SnapshotError::State)?;
    if matches!(store.layout.get(), 4..=7) {
        let mut catalog = super::catalog::load(&tx).map_err(SnapshotError::State)?;
        let pinned = catalog
            .anchor
            .and_then(|s| catalog.slots[usize::from(s)])
            .map(|f| f.0);
        if let Some(id) = expected {
            if catalog.pending.is_some() {
                return Err(SnapshotError::TransitionPending);
            }
            if pinned.is_some_and(|old| old != id) {
                return Err(SnapshotError::SnapshotPinned);
            }
            let slot = catalog
                .find(id)
                .filter(|s| catalog.current == Some(*s))
                .ok_or(SnapshotError::StaleSnapshot)?;
            if pinned.is_none() {
                catalog.anchor = Some(slot);
                super::catalog::put(&tx, &catalog).map_err(SnapshotError::State)?;
                super::commit(tx).map_err(SnapshotError::State)?;
            }
            return Ok(Some(id));
        }
        return Ok(pinned);
    }
    let pinned = validate::pin(&tx).map_err(SnapshotError::State)?;
    if let Some(id) = expected {
        let facts = facts
            .filter(|facts| facts.0 == id)
            .ok_or(SnapshotError::StaleSnapshot)?;
        if pinned.is_none() {
            super::pin_descriptor(&tx, facts).map_err(SnapshotError::State)?;
            super::commit(tx).map_err(SnapshotError::State)?;
        }
        return Ok(Some(id));
    }
    Ok(pinned)
}

pub(super) fn by_id<W: std::io::Write>(
    store: &RedbStateStore,
    id: [u8; 32],
    output: &mut W,
    limits: SnapshotLimits,
) -> Result<Option<SnapshotMetadata>, SnapshotError> {
    let mut tx = super::transaction(store, limits, store.limits).map_err(SnapshotError::State)?;
    let mut charge = PolicySchemaCharge::new(store.limits.schema());
    validate::complete(&mut tx, &mut charge, store.limits.snapshot_limits())
        .map_err(SnapshotError::State)?;
    let catalog = super::catalog::load(&tx).map_err(SnapshotError::State)?;
    let Some(slot) = catalog.find(id) else {
        return Ok(None);
    };
    super::catalog::select(&mut tx, slot);
    let facts = catalog.slots[usize::from(slot)].ok_or(SnapshotError::State(Error::Corrupt))?;
    let metadata = metadata(&tx, facts)?;
    if format::emit(&tx, Some(output), limits)? != facts {
        return Err(SnapshotError::State(Error::Corrupt));
    }
    Ok(Some(metadata))
}
