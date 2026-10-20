//! Consumed candidate custody and one atomic live+artifact publication.

use super::super::store::RedbStateStore;
use super::{
    RedbSnapshotStage, SnapshotError, SnapshotLimits, SnapshotMetadata, read, select, validate,
};
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_domain::workflow::control::EntryIdentity;
use wf_ports::state::StateTransactionError;

pub(super) fn install(
    store: &RedbStateStore,
    stage: RedbSnapshotStage,
    limits: SnapshotLimits,
) -> Result<SnapshotMetadata, SnapshotError> {
    if store.layout.get() >= 5 && stage.store.layout.get() < store.layout.get() {
        return Err(SnapshotError::State(
            StateTransactionError::UnsupportedLayout,
        ));
    }
    if matches!(store.layout.get(), 4..=6) || stage.store.layout.get() >= 5 {
        return advance(store, stage, limits, None)?
            .ok_or(SnapshotError::State(StateTransactionError::Corrupt));
    }
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

pub(super) fn advance(
    store: &RedbStateStore,
    stage: RedbSnapshotStage,
    limits: SnapshotLimits,
    transition: Option<wf_ports::snapshot::SnapshotTransition>,
) -> Result<Option<SnapshotMetadata>, SnapshotError> {
    let source_store = stage.store;
    if store.layout.get() >= 5 && source_store.layout.get() < store.layout.get() {
        return Err(SnapshotError::State(
            StateTransactionError::UnsupportedLayout,
        ));
    }
    if store.group != source_store.group || store.recovery != source_store.recovery {
        return Err(SnapshotError::NamespaceMismatch);
    }
    source_store.ready().map_err(SnapshotError::Stage)?;
    let mut target =
        super::transaction(store, limits, store.limits).map_err(SnapshotError::State)?;
    let mut charge = PolicySchemaCharge::new(store.limits.schema());
    validate::complete(&mut target, &mut charge, store.limits.snapshot_limits())
        .map_err(SnapshotError::State)?;
    let mut catalog = if store.layout.get() == 3 {
        super::policy::publication_catalog(&target)
    } else {
        super::catalog::load(&target)
    }
    .map_err(SnapshotError::State)?;
    if catalog.pending.is_some() && catalog.pending != transition {
        return Err(SnapshotError::TransitionPending);
    }
    if let Some(t) = transition {
        if !t.is_received() {
            return Err(SnapshotError::TransitionConflict);
        }
        super::transition::prior(&mut target, &catalog, t)?;
    }
    let mut source =
        super::transaction(&source_store, limits, store.limits).map_err(SnapshotError::Stage)?;
    source.carry_from(&target);
    let facts = validate::complete(&mut source, &mut charge, limits)
        .map_err(SnapshotError::Stage)?
        .ok_or(SnapshotError::Stage(StateTransactionError::Corrupt))?;
    if let Some(t) = transition {
        use wf_ports::state::GroupApplyTransaction;
        if facts.0 != *t.id()
            || source
                .control()
                .map_err(SnapshotError::Stage)?
                .last_entry()
                .map(EntryIdentity::log)
                != Some(t.position())
        {
            return Err(SnapshotError::TransitionConflict);
        }
    }
    if catalog.pending.is_some() {
        return Ok(None);
    }
    target.carry_from(&source);
    validate::compatible(&source, &target)?;
    target.carry_from(&source);
    if store.layout.get() == 3 {
        super::policy::upgrade_catalog(&target, &catalog).map_err(SnapshotError::State)?;
    }
    let existing = catalog.find(facts.0);
    let slot = existing.unwrap_or_else(|| catalog.destination());
    for table in [super::super::META, super::super::ROWS] {
        super::retire(&target, table).map_err(SnapshotError::State)?;
    }
    super::copy(&source, &target, super::super::META, super::super::META)?;
    super::copy(&source, &target, super::super::ROWS, super::super::ROWS)?;
    if source_store.layout.get() < 5 {
        super::format::set_manifest(&target, 4).map_err(SnapshotError::State)?;
    }
    if existing.is_none() {
        super::catalog::retire(&target, slot).map_err(SnapshotError::State)?;
        let [meta, rows] = super::catalog::tables(slot);
        super::copy(&source, &target, super::META, meta)?;
        super::copy(&source, &target, super::ROWS, rows)?;
    }
    catalog.current = Some(slot);
    catalog.slots[usize::from(slot)] = Some(facts);
    catalog.pending = transition;
    super::catalog::put(&target, &catalog).map_err(SnapshotError::State)?;
    if validate::complete(&mut target, &mut charge, store.limits.snapshot_limits())
        .map_err(SnapshotError::State)?
        != Some(facts)
    {
        return Err(SnapshotError::State(StateTransactionError::Corrupt));
    }
    let metadata = if transition.is_none() {
        super::catalog::select(&mut target, slot);
        Some(read::metadata(&target, facts)?)
    } else {
        None
    };
    super::commit(target).map_err(SnapshotError::State)?;
    Ok(metadata)
}
