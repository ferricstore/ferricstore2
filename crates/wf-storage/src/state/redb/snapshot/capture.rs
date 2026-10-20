//! One serialized writer; sink completion precedes durable artifact publication.

use super::super::{recovery, store::RedbStateStore};
use super::{SnapshotError, SnapshotLimits, SnapshotMetadata, format, read, select, validate};
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_ports::state::StateTransactionError;

fn actual(error: SnapshotError) -> SnapshotError {
    match error {
        SnapshotError::Stage(error) => SnapshotError::State(error),
        other => other,
    }
}

pub(super) fn capture<W: std::io::Write>(
    store: &RedbStateStore,
    output: &mut W,
    limits: SnapshotLimits,
) -> Result<SnapshotMetadata, SnapshotError> {
    if matches!(store.layout.get(), 4..=6) {
        return advance(store, output, limits);
    }
    let mut tx = super::transaction(store, limits, store.limits).map_err(SnapshotError::State)?;
    let mut charge = PolicySchemaCharge::new(store.limits.schema());
    let _ = validate::complete(&mut tx, &mut charge, store.limits.snapshot_limits())
        .map_err(SnapshotError::State)?;
    if validate::pin(&tx).map_err(SnapshotError::State)?.is_some() {
        return Err(SnapshotError::SnapshotPinned);
    }
    // New DATA caps never reinterpret an older, larger artifact's valid profile.
    let expected = format::facts(&tx, limits).map_err(SnapshotError::State)?;
    for table in [super::META, super::ROWS] {
        super::retire(&tx, table).map_err(SnapshotError::State)?;
    }
    super::copy(&tx, &tx, super::super::META, super::META).map_err(actual)?;
    super::copy(&tx, &tx, super::super::ROWS, super::ROWS).map_err(actual)?;
    select(&mut tx, true);
    if recovery::cohort(&tx, &mut charge)
        .map_err(SnapshotError::State)?
        .0
        != 3
        || format::facts(&tx, limits).map_err(SnapshotError::State)? != expected
    {
        return Err(SnapshotError::State(StateTransactionError::Corrupt));
    }
    super::put_descriptor(&tx, Some(expected)).map_err(SnapshotError::State)?;
    let metadata = read::metadata(&tx, expected)?;
    if format::emit(&tx, Some(output), limits)? != expected {
        return Err(SnapshotError::State(StateTransactionError::Corrupt));
    }
    super::commit(tx).map_err(SnapshotError::State)?;
    Ok(metadata)
}

fn advance<W: std::io::Write>(
    store: &RedbStateStore,
    output: &mut W,
    limits: SnapshotLimits,
) -> Result<SnapshotMetadata, SnapshotError> {
    let mut tx = super::transaction(store, limits, store.limits).map_err(SnapshotError::State)?;
    let mut charge = PolicySchemaCharge::new(store.limits.schema());
    validate::complete(&mut tx, &mut charge, store.limits.snapshot_limits())
        .map_err(SnapshotError::State)?;
    let mut catalog = super::catalog::load(&tx).map_err(SnapshotError::State)?;
    if catalog.pending.is_some() {
        return Err(SnapshotError::TransitionPending);
    }
    let expected = format::facts(&tx, limits).map_err(SnapshotError::State)?;
    let slot = if let Some(existing) = catalog.find(expected.0) {
        existing
    } else {
        let destination = catalog.destination();
        super::catalog::retire(&tx, destination).map_err(SnapshotError::State)?;
        let [meta, rows] = super::catalog::tables(destination);
        super::copy(&tx, &tx, super::super::META, meta).map_err(actual)?;
        super::copy(&tx, &tx, super::super::ROWS, rows).map_err(actual)?;
        catalog.slots[usize::from(destination)] = Some(expected);
        destination
    };
    catalog.current = Some(slot);
    super::catalog::put(&tx, &catalog).map_err(SnapshotError::State)?;
    validate::complete(&mut tx, &mut charge, store.limits.snapshot_limits())
        .map_err(SnapshotError::State)?;
    super::catalog::select(&mut tx, slot);
    let metadata = read::metadata(&tx, expected)?;
    if format::emit(&tx, Some(output), limits)? != expected {
        return Err(SnapshotError::State(StateTransactionError::Corrupt));
    }
    super::commit(tx).map_err(SnapshotError::State)?;
    Ok(metadata)
}
