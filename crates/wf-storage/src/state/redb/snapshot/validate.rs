//! One shared semantic validator, explicit lineage and indexed historical data.

use super::super::{membership, recovery, store::RedbStateTransaction};
use super::{KEY, META, SnapshotError, SnapshotLimits, admission, format, select};
use crate::state::value;
use redb::ReadableTable;
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_domain::workflow::control::ControlState;
use wf_ports::state::{GroupApplyTransaction, StateTransactionError as Error};

pub(super) fn marker(
    tx: &RedbStateTransaction<'_>,
) -> Result<Option<([u8; 32], usize, usize)>, Error> {
    tx.step()?;
    let table = tx
        .write
        .open_table(super::super::META)
        .map_err(admission::table_error)?;
    tx.step()?;
    let bytes = table
        .get(KEY)
        .map_err(|e| admission::storage_error(&e))?
        .ok_or(Error::Corrupt)?;
    tx.recovery_row(KEY, bytes.value())?;
    format::decode_descriptor(tx, bytes.value())
}

pub(super) fn pin(tx: &RedbStateTransaction<'_>) -> Result<Option<[u8; 32]>, Error> {
    tx.step()?;
    let table = tx
        .write
        .open_table(super::super::META)
        .map_err(admission::table_error)?;
    tx.step()?;
    let bytes = table
        .get(KEY)
        .map_err(|error| admission::storage_error(&error))?
        .ok_or(Error::Corrupt)?;
    tx.recovery_row(KEY, bytes.value())?;
    let facts = format::decode_descriptor(tx, bytes.value())?;
    Ok(if bytes.value()[5] == 2 {
        facts.map(|value| value.0)
    } else {
        None
    })
}

pub(super) fn empty(tx: &RedbStateTransaction<'_>) -> Result<(), Error> {
    for definition in [tx.metadata_table, tx.rows_table] {
        tx.step()?;
        let table = tx
            .write
            .open_table(definition)
            .map_err(admission::table_error)?;
        tx.step()?;
        let mut rows = table.iter().map_err(|e| admission::storage_error(&e))?;
        tx.step()?;
        if let Some(row) = rows.next() {
            let (key, value) = row.map_err(|e| admission::storage_error(&e))?;
            tx.recovery_row(key.value(), value.value())?;
            return Err(Error::Corrupt);
        }
    }
    Ok(())
}

pub(in crate::state::redb) fn retained(
    tx: &mut RedbStateTransaction<'_>,
    charge: &mut PolicySchemaCharge,
    profile: SnapshotLimits,
    layout: u16,
) -> Result<Option<([u8; 32], usize, usize)>, Error> {
    if matches!(layout, 4..=7) {
        let supported = tx.limits.snapshot_limits();
        return super::catalog::retained(tx, charge, supported);
    }
    let current = tx.control()?;
    let Some(expected) = marker(tx)? else {
        select(tx, true);
        let result = empty(tx);
        select(tx, false);
        result?;
        return Ok(None);
    };
    select(tx, true);
    let result = retained_inner(tx, charge, profile, current, expected);
    select(tx, false);
    result.map(|()| Some(expected))
}

fn retained_inner(
    tx: &RedbStateTransaction<'_>,
    charge: &mut PolicySchemaCharge,
    profile: SnapshotLimits,
    current: ControlState,
    expected: ([u8; 32], usize, usize),
) -> Result<(), Error> {
    let (layout, saved) = recovery::cohort(tx, charge)?;
    if !matches!(layout, 3 | 5..=7) || format::facts(tx, profile)? != expected {
        return Err(Error::Corrupt);
    }
    tx.step()?;
    let old = tx.write.open_table(META).map_err(admission::table_error)?;
    tx.step()?;
    let live = tx
        .write
        .open_table(super::super::META)
        .map_err(admission::table_error)?;
    let old_manifest = get(tx, &old, b"manifest")?;
    let live_manifest = get(tx, &live, b"manifest")?;
    let old_origin = super::policy::origin(tx, &old, old_manifest.value())?;
    let live_origin = super::policy::origin(tx, &live, live_manifest.value())?;
    let old_member = get(tx, &old, membership::KEY)?;
    let live_member = get(tx, &live, membership::KEY)?;
    values(
        tx,
        (saved, current),
        (
            old_origin
                .as_ref()
                .map_or(old_manifest.value(), redb::AccessGuard::value),
            live_origin
                .as_ref()
                .map_or(live_manifest.value(), redb::AccessGuard::value),
        ),
        (old_member.value(), live_member.value()),
    )
    .map_err(|_| Error::Corrupt)
}

pub(super) fn get<'a>(
    tx: &RedbStateTransaction<'_>,
    table: &'a impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &[u8],
) -> Result<redb::AccessGuard<'a, &'static [u8]>, Error> {
    tx.step()?;
    let bytes = table
        .get(key)
        .map_err(|e| admission::storage_error(&e))?
        .ok_or(Error::Corrupt)?;
    tx.recovery_row(key, bytes.value())?;
    Ok(bytes)
}

pub(super) fn values(
    tx: &RedbStateTransaction<'_>,
    controls: (ControlState, ControlState),
    manifests: (&[u8], &[u8]),
    members: (&[u8], &[u8]),
) -> Result<(), SnapshotError> {
    if !format::same_genesis(manifests.0, manifests.1) {
        return Err(SnapshotError::GenesisMismatch);
    }
    let (old, new) = controls;
    if new.time().hlc() < old.time().hlc() || new.time().wall() < old.time().wall() {
        return Err(SnapshotError::StaleSnapshot);
    }
    if let Some(prior) = old.last_entry() {
        let next = new.last_entry().ok_or(SnapshotError::StaleSnapshot)?;
        if next.log().index() < prior.log().index()
            || (next.log().index() == prior.log().index() && new != old)
        {
            return Err(SnapshotError::StaleSnapshot);
        }
    } else if new.last_entry().is_none() && new != old {
        return Err(SnapshotError::StaleSnapshot);
    }
    let profile = tx.limits.membership_limits();
    let mut owner = 0;
    let before = value::inspect_membership(members.0, tx.group, tx.recovery, profile, &mut owner)
        .map_err(admission::value_error)
        .map_err(SnapshotError::Stage)?;
    let after = value::inspect_membership(members.1, tx.group, tx.recovery, profile, &mut owner)
        .map_err(admission::value_error)
        .map_err(SnapshotError::Stage)?;
    if let Some((prior, _, _)) = before {
        let next = after.ok_or(SnapshotError::MembershipMismatch)?.0;
        if next.log().index() < prior.log().index()
            || (next.log().index() == prior.log().index() && members.0 != members.1)
        {
            return Err(SnapshotError::MembershipMismatch);
        }
    }
    if old.last_entry() == new.last_entry() && members.0 != members.1 {
        return Err(SnapshotError::MembershipMismatch);
    }
    Ok(())
}

pub(super) fn compatible(
    source: &RedbStateTransaction<'_>,
    target: &RedbStateTransaction<'_>,
) -> Result<(), SnapshotError> {
    source.carry_from(target);
    let incoming = source.control().map_err(SnapshotError::Stage)?;
    source.step().map_err(SnapshotError::Stage)?;
    let source_table = source
        .write
        .open_table(source.metadata_table)
        .map_err(admission::table_error)
        .map_err(SnapshotError::Stage)?;
    let new_manifest = get(source, &source_table, b"manifest").map_err(SnapshotError::Stage)?;
    let new_origin = super::policy::origin(source, &source_table, new_manifest.value())
        .map_err(SnapshotError::Stage)?;
    let new_member = get(source, &source_table, membership::KEY).map_err(SnapshotError::Stage)?;
    target.carry_from(source);
    let current = target.control().map_err(SnapshotError::State)?;
    target.step().map_err(SnapshotError::State)?;
    let target_table = target
        .write
        .open_table(target.metadata_table)
        .map_err(admission::table_error)
        .map_err(SnapshotError::State)?;
    let old_manifest = get(target, &target_table, b"manifest").map_err(SnapshotError::State)?;
    let old_origin = super::policy::origin(target, &target_table, old_manifest.value())
        .map_err(SnapshotError::State)?;
    let old_member = get(target, &target_table, membership::KEY).map_err(SnapshotError::State)?;
    source.carry_from(target);
    value_defaults_compatible(source, old_manifest.value(), new_manifest.value())?;
    values(
        source,
        (current, incoming),
        (
            old_origin
                .as_ref()
                .map_or(old_manifest.value(), redb::AccessGuard::value),
            new_origin
                .as_ref()
                .map_or(new_manifest.value(), redb::AccessGuard::value),
        ),
        (old_member.value(), new_member.value()),
    )
}

pub(super) fn value_defaults_compatible(
    tx: &RedbStateTransaction<'_>,
    old: &[u8],
    new: &[u8],
) -> Result<(), SnapshotError> {
    let old_value = old.get(4..6) == Some([0, 7].as_slice());
    let new_value = new.get(4..6) == Some([0, 7].as_slice());
    if old_value != new_value {
        return Err(SnapshotError::GenesisMismatch);
    }
    if old_value {
        let a = super::super::values::manifest::parse(old, tx.group, tx.recovery)
            .map_err(SnapshotError::State)?;
        let b = super::super::values::manifest::parse(new, tx.group, tx.recovery)
            .map_err(SnapshotError::Stage)?;
        if a.defaults != b.defaults || a.alias_format != b.alias_format {
            return Err(SnapshotError::GenesisMismatch);
        }
    }
    Ok(())
}

pub(super) fn complete(
    tx: &mut RedbStateTransaction<'_>,
    charge: &mut PolicySchemaCharge,
    profile: SnapshotLimits,
) -> Result<Option<([u8; 32], usize, usize)>, Error> {
    let tables = recovery::inventory(tx)?;
    let layout = recovery::cohort(tx, charge)?.0;
    if layout < 3 {
        return Err(Error::UnsupportedLayout);
    }
    if tables != if matches!(layout, 4..=7) { 6 } else { 4 } {
        return Err(Error::Corrupt);
    }
    retained(tx, charge, profile, layout)
}
