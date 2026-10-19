//! Consumed candidate custody and one atomic live+artifact publication.

use super::super::store::RedbStateStore;
use super::{
    RedbSnapshotStage, SnapshotError, SnapshotLimits, SnapshotMetadata, read, select, validate,
};
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_ports::state::StateTransactionError;

pub(super) fn install(
    store: &RedbStateStore,
    stage: RedbSnapshotStage,
    limits: SnapshotLimits,
) -> Result<SnapshotMetadata, SnapshotError> {
    let source_store = stage.store;
    if store.group != source_store.group || store.recovery != source_store.recovery {
        return Err(SnapshotError::NamespaceMismatch);
    }
    source_store.ready().map_err(SnapshotError::Stage)?;
    let mut target =
        super::transaction(store, limits, store.limits).map_err(SnapshotError::State)?;
    let mut charge = PolicySchemaCharge::new(store.limits.schema());
    let _ = validate::complete(&mut target, &mut charge, store.limits.snapshot_limits())
        .map_err(SnapshotError::State)?;
    if validate::pin(&target)
        .map_err(SnapshotError::State)?
        .is_some()
    {
        return Err(SnapshotError::SnapshotPinned);
    }
    // Rebind actual incoming content to the TARGET profile, not the old stage cap.
    let mut source =
        super::transaction(&source_store, limits, store.limits).map_err(SnapshotError::Stage)?;
    source.carry_from(&target);
    let facts = validate::complete(&mut source, &mut charge, limits)
        .map_err(SnapshotError::Stage)?
        .ok_or(SnapshotError::Stage(StateTransactionError::Corrupt))?;
    target.carry_from(&source);
    validate::compatible(&source, &target)?;
    target.carry_from(&source);
    for table in [
        super::super::META,
        super::super::ROWS,
        super::META,
        super::ROWS,
    ] {
        super::retire(&target, table).map_err(SnapshotError::State)?;
    }
    for table in [
        super::super::META,
        super::super::ROWS,
        super::META,
        super::ROWS,
    ] {
        super::copy(&source, &target, table, table)?;
    }
    if validate::complete(&mut target, &mut charge, limits).map_err(SnapshotError::State)?
        != Some(facts)
    {
        return Err(SnapshotError::State(StateTransactionError::Corrupt));
    }
    select(&mut target, true);
    let metadata = read::metadata(&target, facts)?;
    super::commit(target).map_err(SnapshotError::State)?;
    Ok(metadata)
}
