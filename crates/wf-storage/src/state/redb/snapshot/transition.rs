//! One state transaction per durable prepare or acknowledged retirement.

use super::super::store::{RedbStateStore, RedbStateTransaction};
use super::{
    RedbSnapshotStage, SnapshotError, SnapshotLimits, SnapshotMetadata, active, admit, catalog,
    read, validate,
};
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_domain::workflow::control::EntryIdentity;
use wf_ports::{
    log::RaftLogState,
    snapshot::{SnapshotTransition, StateSnapshotTransitionStore},
    state::{GroupApplyTransaction, StateTransactionError as Error},
};

fn mode(store: &RedbStateStore, limits: SnapshotLimits, extra: usize) -> Result<(), SnapshotError> {
    store.ready().map_err(SnapshotError::State)?;
    if !matches!(store.layout.get(), 4..=8) {
        return Err(SnapshotError::State(Error::UnsupportedLayout));
    }
    admit(&store.group, &store.recovery, store.limits, limits, extra)
}

pub(super) fn prior(
    tx: &mut RedbStateTransaction<'_>,
    catalog: &catalog::SnapshotCatalog,
    t: SnapshotTransition,
) -> Result<(), SnapshotError> {
    if let Some(p) = t.before().last_purged() {
        let slot = catalog.anchor.ok_or(SnapshotError::TransitionConflict)?;
        if catalog.slots[usize::from(slot)].map(|f| f.0) != t.before().purged_snapshot_id() {
            return Err(SnapshotError::TransitionConflict);
        }
        catalog::select(tx, slot);
        if tx
            .control()
            .map_err(SnapshotError::State)?
            .last_entry()
            .map(EntryIdentity::log)
            != Some(p)
        {
            return Err(SnapshotError::TransitionConflict);
        }
        super::select(tx, false);
    }
    Ok(())
}

fn observe(
    store: &RedbStateStore,
    limits: SnapshotLimits,
) -> Result<Option<SnapshotTransition>, SnapshotError> {
    let mut tx = super::transaction(store, limits, store.limits).map_err(SnapshotError::State)?;
    let mut charge = PolicySchemaCharge::new(store.limits.schema());
    validate::complete(&mut tx, &mut charge, store.limits.snapshot_limits())
        .map_err(SnapshotError::State)?;
    Ok(catalog::load(&tx).map_err(SnapshotError::State)?.pending)
}

fn prepare(
    store: &RedbStateStore,
    t: SnapshotTransition,
    limits: SnapshotLimits,
) -> Result<(), SnapshotError> {
    let mut tx = super::transaction(store, limits, store.limits).map_err(SnapshotError::State)?;
    let mut charge = PolicySchemaCharge::new(store.limits.schema());
    validate::complete(&mut tx, &mut charge, store.limits.snapshot_limits())
        .map_err(SnapshotError::State)?;
    let mut catalog = catalog::load(&tx).map_err(SnapshotError::State)?;
    if let Some(old) = catalog.pending {
        if old != t {
            return Err(SnapshotError::TransitionPending);
        }
        catalog::require(&mut tx, &catalog, t, limits)?;
        return Ok(());
    }
    prior(&mut tx, &catalog, t)?;
    catalog::require(&mut tx, &catalog, t, limits)?;
    catalog.pending = Some(t);
    catalog::put(&tx, &catalog).map_err(SnapshotError::State)?;
    validate::complete(&mut tx, &mut charge, store.limits.snapshot_limits())
        .map_err(SnapshotError::State)?;
    super::commit(tx).map_err(SnapshotError::State)
}

fn finish(
    store: &RedbStateStore,
    observed: RaftLogState,
    limits: SnapshotLimits,
) -> Result<(), SnapshotError> {
    let mut tx = super::transaction(store, limits, store.limits).map_err(SnapshotError::State)?;
    let mut charge = PolicySchemaCharge::new(store.limits.schema());
    validate::complete(&mut tx, &mut charge, store.limits.snapshot_limits())
        .map_err(SnapshotError::State)?;
    let mut catalog = catalog::load(&tx).map_err(SnapshotError::State)?;
    let target = catalog.current.ok_or(SnapshotError::TransitionConflict)?;
    let facts = catalog.slots[usize::from(target)].ok_or(SnapshotError::TransitionConflict)?;
    catalog::select(&mut tx, target);
    let p = tx
        .control()
        .map_err(SnapshotError::State)?
        .last_entry()
        .ok_or(SnapshotError::TransitionConflict)?
        .log();
    super::select(&mut tx, false);
    if observed.last_purged() != Some(p) || observed.purged_snapshot_id() != Some(facts.0) {
        return Err(SnapshotError::TransitionConflict);
    }
    let Some(t) = catalog.pending else {
        return if catalog.anchor == Some(target) {
            Ok(())
        } else {
            Err(SnapshotError::TransitionConflict)
        };
    };
    if observed.vote() != t.before().vote()
        || (!t.is_received()
            && (observed.committed() != t.before().committed()
                || observed.last_log() != t.before().last_log()))
        || (t.is_received()
            && (observed.committed() != Some(t.position())
                || (observed.last_log() != Some(t.position())
                    && observed.last_log() != t.before().last_log())))
    {
        return Err(SnapshotError::TransitionConflict);
    }
    if let Some(old) = catalog.anchor.filter(|&s| s != target) {
        catalog::retire(&tx, old).map_err(SnapshotError::State)?;
        catalog.slots[usize::from(old)] = None;
    }
    catalog.anchor = Some(target);
    catalog.pending = None;
    catalog::put(&tx, &catalog).map_err(SnapshotError::State)?;
    validate::complete(&mut tx, &mut charge, store.limits.snapshot_limits())
        .map_err(SnapshotError::State)?;
    super::commit(tx).map_err(SnapshotError::State)
}

impl StateSnapshotTransitionStore for RedbStateStore {
    fn supports_snapshot_transitions(&self) -> Result<bool, SnapshotError> {
        self.ready().map_err(SnapshotError::State)?;
        Ok(matches!(self.layout.get(), 4..=8))
    }
    fn snapshot_by_id<W: std::io::Write>(
        &mut self,
        id: [u8; 32],
        output: &mut W,
        limits: SnapshotLimits,
    ) -> Result<Option<SnapshotMetadata>, SnapshotError> {
        mode(self, limits, 0)?;
        active(self, read::by_id(self, id, output, limits))
    }
    fn snapshot_transition(
        &mut self,
        limits: SnapshotLimits,
    ) -> Result<Option<SnapshotTransition>, SnapshotError> {
        mode(self, limits, 0)?;
        active(self, observe(self, limits))
    }
    fn prepare_snapshot_transition(
        &mut self,
        t: SnapshotTransition,
        limits: SnapshotLimits,
    ) -> Result<(), SnapshotError> {
        mode(self, limits, 0)?;
        active(self, prepare(self, t, limits))
    }
    fn prepare_received_snapshot(
        &mut self,
        stage: RedbSnapshotStage,
        t: SnapshotTransition,
        limits: SnapshotLimits,
    ) -> Result<(), SnapshotError> {
        self.ready().map_err(SnapshotError::State)?;
        if !matches!(self.layout.get(), 4..=8) {
            return Err(SnapshotError::State(Error::UnsupportedLayout));
        }
        let extra = super::admission::total(
            512,
            [
                stage.group().as_bytes().len(),
                stage.recovery().as_bytes().len(),
            ],
        )
        .map_err(|_| SnapshotError::Limit)?;
        mode(self, limits, extra)?;
        let incoming_layout = stage.store.layout.get();
        active(self, super::install::advance(self, stage, limits, Some(t)))?;
        if incoming_layout >= 5 {
            self.layout.set(incoming_layout);
        }
        Ok(())
    }
    fn finish_snapshot_transition(
        &mut self,
        log: RaftLogState,
        limits: SnapshotLimits,
    ) -> Result<(), SnapshotError> {
        mode(self, limits, 0)?;
        active(self, finish(self, log, limits))
    }
}
