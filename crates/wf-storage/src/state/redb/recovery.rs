//! Exclusive noncreating recovery with a cumulative census and shared grammar.

use super::{
    admission::{storage_error, table_error, total, value_error},
    bootstrap::hash_row,
    store::{RedbStateStore, RedbStateTransaction},
};
use crate::state::{key, value};
use redb::{ReadableTable, TableHandle};
use sha2::{Digest, Sha256};
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_ports::state::{GroupApplyTransaction, StateTransactionError};
use wf_types::version::{CommittedTime, HlcTimestamp};

mod index;
mod rows;

pub(super) fn validate(store: &RedbStateStore) -> Result<(u16, u16), StateTransactionError> {
    let mut tx = store.transaction(
        true,
        store.limits.admission(),
        store.limits.maximum_recovery_steps(),
        usize::MAX,
        store.limits.maximum_temporary_bytes(),
    )?;
    let tables = inventory(&tx)?;
    let mut charge = PolicySchemaCharge::new(tx.limits.schema());
    let (layout, _, catalog_profile) = cohort_profiled(&tx, &mut charge)?;
    if layout < 3 {
        if tables != 2 {
            return Err(StateTransactionError::Corrupt);
        }
        return Err(StateTransactionError::UnsupportedLayout);
    }
    if tables != if matches!(layout, 4..=8) { 6 } else { 4 } {
        return Err(StateTransactionError::Corrupt);
    }
    let _ = super::snapshot::validate::retained(
        &mut tx,
        &mut charge,
        store.limits.snapshot_limits(),
        layout,
    )?;
    Ok((layout, catalog_profile))
}

pub(super) fn cohort(
    tx: &RedbStateTransaction<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<(u16, wf_domain::workflow::control::ControlState), StateTransactionError> {
    cohort_profiled(tx, charge).map(|(layout, control, _)| (layout, control))
}

pub(super) fn cohort_profiled(
    tx: &RedbStateTransaction<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<(u16, wf_domain::workflow::control::ControlState, u16), StateTransactionError> {
    tx.step()?;
    let metadata = tx
        .write
        .open_table(tx.metadata_table)
        .map_err(table_error)?;
    tx.step()?;
    let mut meta_rows = metadata.iter().map_err(|e| storage_error(&e))?;
    tx.step()?;
    let (meta_key, manifest) = meta_rows
        .next()
        .transpose()
        .map_err(|e| storage_error(&e))?
        .ok_or(StateTransactionError::Corrupt)?;
    tx.recovery_row(meta_key.value(), manifest.value())?;
    if meta_key.value() != b"manifest" {
        return Err(StateTransactionError::Corrupt);
    }
    let mut layout = 0;
    let (initial, placements, policies, expected) =
        check_manifest(manifest.value(), tx, &mut layout)?;
    let member = if layout >= 2 {
        tx.step()?;
        let (key, bytes) = meta_rows
            .next()
            .transpose()
            .map_err(|e| storage_error(&e))?
            .ok_or(StateTransactionError::Corrupt)?;
        tx.recovery_row(key.value(), bytes.value())?;
        if key.value() != super::membership::KEY {
            return Err(StateTransactionError::Corrupt);
        }
        Some(bytes)
    } else {
        None
    };
    if matches!(layout, 5..=8) {
        tx.step()?;
        let (key, origin) = meta_rows
            .next()
            .transpose()
            .map_err(|e| storage_error(&e))?
            .ok_or(StateTransactionError::Corrupt)?;
        tx.recovery_row(key.value(), origin.value())?;
        if key.value() != b"origin" {
            return Err(StateTransactionError::Corrupt);
        }
        let current = super::core_policy::catalog::parse5(tx, manifest.value())?;
        super::core_policy::catalog::bind_origin(tx, current, origin.value())?;
    }
    if layout >= 3 && !tx.snapshot_cohort {
        tx.step()?;
        let (key, bytes) = meta_rows
            .next()
            .transpose()
            .map_err(|e| storage_error(&e))?
            .ok_or(StateTransactionError::Corrupt)?;
        tx.recovery_row(key.value(), bytes.value())?;
        if key.value() != super::snapshot::KEY {
            return Err(StateTransactionError::Corrupt);
        }
        if (matches!(layout, 4..=8)) != bytes.value().starts_with(b"FWSD\0\x03") {
            return Err(StateTransactionError::Corrupt);
        }
        super::snapshot::check_descriptor(tx, bytes.value())?;
    }
    tx.step()?;
    if let Some(row) = meta_rows.next() {
        let (key, value) = row.map_err(|e| storage_error(&e))?;
        tx.recovery_row(key.value(), value.value())?;
        return Err(StateTransactionError::Corrupt);
    }
    let control = tx.control()?;
    if matches!(layout, 5 | 6) && control.last_entry().is_none() {
        return Err(StateTransactionError::Corrupt);
    }
    if control.last_entry().is_none() && control.time() != initial {
        return Err(StateTransactionError::Corrupt);
    }
    if control.time().hlc() < initial.hlc() || control.time().wall() < initial.wall() {
        return Err(StateTransactionError::Corrupt);
    }
    if let Some(bytes) = member {
        super::membership::validate_value(tx, bytes.value(), control)?;
    }
    let (actual_placements, actual_policies, hash) =
        validate_rows(tx, control, (layout, manifest.value()), charge)?;
    if placements != actual_placements || policies != actual_policies || expected != hash {
        return Err(StateTransactionError::Corrupt);
    }
    let catalog_profile = validate_values_profile(tx, manifest.value(), layout)?;
    Ok((layout, control, catalog_profile))
}

fn validate_values_profile(
    tx: &RedbStateTransaction<'_>,
    manifest: &[u8],
    layout: u16,
) -> Result<u16, StateTransactionError> {
    if !matches!(layout, 7 | 8) {
        return Ok(0);
    }
    let values = super::values::manifest::parse(manifest, tx.group, tx.recovery)?;
    super::values::validate::cohort(tx, values.defaults, values.alias_format, layout)?;
    Ok(values.governance_catalog.version())
}

pub(super) fn inventory(tx: &RedbStateTransaction<'_>) -> Result<usize, StateTransactionError> {
    tx.step()?;
    let mut tables = tx.write.list_tables().map_err(|e| storage_error(&e))?;
    let mut count = 0;
    let mut mask = 0_u8;
    loop {
        tx.step()?;
        let Some(table) = tables.next() else {
            break;
        };
        mask |= match table.name() {
            "wf_state_meta_v1" => 1,
            "wf_state_rows_v1" => 2,
            "wf_state_snapshot_meta_v1" => 4,
            "wf_state_snapshot_rows_v1" => 8,
            "wf_state_snapshot_alt_meta_v1" => 16,
            "wf_state_snapshot_alt_rows_v1" => 32,
            _ => return Err(StateTransactionError::Corrupt),
        };
        count += 1;
    }
    tx.step()?;
    let mut multimaps = tx
        .write
        .list_multimap_tables()
        .map_err(|e| storage_error(&e))?;
    tx.step()?;
    if !matches!((count, mask), (2, 3) | (4, 15) | (6, 63)) || multimaps.next().is_some() {
        return Err(StateTransactionError::Corrupt);
    }
    Ok(count)
}

pub(super) fn check_manifest(
    bytes: &[u8],
    tx: &RedbStateTransaction<'_>,
    layout: &mut u16,
) -> Result<(CommittedTime, u64, u64, [u8; 32]), StateTransactionError> {
    let mut at = 0;
    if value::take(bytes, &mut at, 4).map_err(value_error)? != b"FWSS" {
        return Err(StateTransactionError::Corrupt);
    }
    *layout = value::read_u16(bytes, &mut at).map_err(value_error)?;
    if matches!(*layout, 5..=8) {
        return super::core_policy::catalog::parse5(tx, bytes)
            .map(super::core_policy::catalog::facts);
    }
    if !matches!(*layout, 1..=4) {
        return Err(StateTransactionError::Corrupt);
    }
    let n = usize::from(value::read_u16(bytes, &mut at).map_err(value_error)?);
    let group = value::take(bytes, &mut at, n).map_err(value_error)?;
    let n = usize::from(value::read_u16(bytes, &mut at).map_err(value_error)?);
    let recovery = value::take(bytes, &mut at, n).map_err(value_error)?;
    let physical = value::read_u64(bytes, &mut at).map_err(value_error)?;
    let logical = value::read_u16(bytes, &mut at).map_err(value_error)?;
    let wall = value::read_u64(bytes, &mut at).map_err(value_error)?;
    let initial = CommittedTime::new(
        HlcTimestamp::new(physical, logical).map_err(|_| StateTransactionError::Corrupt)?,
        wall,
    )
    .map_err(|_| StateTransactionError::Corrupt)?;
    let placements = value::read_u64(bytes, &mut at).map_err(value_error)?;
    let policies = value::read_u64(bytes, &mut at).map_err(value_error)?;
    let hash: [u8; 32] = value::take(bytes, &mut at, 32)
        .map_err(value_error)?
        .try_into()
        .map_err(|_| StateTransactionError::Corrupt)?;
    let checksum = Sha256::digest(&bytes[..at]);
    if value::take(bytes, &mut at, 32).map_err(value_error)? != checksum.as_slice() {
        return Err(StateTransactionError::Corrupt);
    }
    value::finish(bytes, at).map_err(value_error)?;
    if group.is_empty() || group.len() > 256 || recovery.is_empty() || recovery.len() > 256 {
        return Err(StateTransactionError::Corrupt);
    }
    if group != tx.group.as_bytes() || recovery != tx.recovery.as_bytes() {
        return Err(StateTransactionError::NamespaceConflict);
    }
    Ok((initial, placements, policies, hash))
}

fn manifest_values(
    tx: &RedbStateTransaction<'_>,
    layout: u16,
    manifest: &[u8],
) -> Result<Option<super::values::manifest::ValueManifest>, StateTransactionError> {
    if layout == 8 {
        super::values::manifest::parse(manifest, tx.group, tx.recovery).map(Some)
    } else {
        Ok(None)
    }
}

fn validate_rows(
    tx: &RedbStateTransaction<'_>,
    control: wf_domain::workflow::control::ControlState,
    (layout, manifest): (u16, &[u8]),
    charge: &mut PolicySchemaCharge,
) -> Result<(u64, u64, [u8; 32]), StateTransactionError> {
    tx.step()?;
    let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    tx.step()?;
    let mut entries = table.iter().map_err(|e| storage_error(&e))?;
    let mut counts = [0_u64; 2];
    let values = manifest_values(tx, layout, manifest)?;
    let credit_shards = values.and_then(|facts| facts.credit_shard_count);
    let mut credit_counts = [0_u64; 2];
    let credit_catalog = values.is_some_and(|facts| facts.credit_catalog);
    let governance_catalog = values.map_or(
        super::values::manifest::GovernanceCatalogProfile::Disabled,
        |facts| facts.governance_catalog,
    );
    let applied = control.last_entry().is_some();
    let mut hash = Sha256::new();
    hash.update(if matches!(layout, 6..=8) {
        super::snapshot::original::CATALOG_DOMAIN
    } else if layout == 5 {
        super::core_policy::catalog::CURRENT_CATALOG_DOMAIN
    } else {
        b"wf-state-genesis-v1"
    });
    loop {
        tx.step()?;
        let Some(row) = entries.next() else {
            break;
        };
        let (physical, value) = row.map_err(|e| storage_error(&e))?;
        tx.recovery_row(physical.value(), value.value())?;
        let maximum = total(256, [physical.value().len()])?;
        tx.temporary(maximum)?;
        let decoded = key::decode_state_key(
            physical.value(),
            tx.group,
            tx.recovery,
            physical.value().len(),
            maximum,
        )
        .map_err(value_error)?;
        if !applied && matches!(decoded.family(), 1 | 2 | 5 | 8..=24) {
            return Err(StateTransactionError::Corrupt);
        }
        if matches!(decoded.family(), 20..=24) {
            super::release::recovery::row(
                tx,
                &table,
                &decoded,
                value.value(),
                credit_shards.ok_or(StateTransactionError::Corrupt)?,
                control,
                governance_catalog,
            )?;
        } else if matches!(decoded.family(), 18 | 19) {
            rows::governance(decoded.family(), value.value(), governance_catalog, control)?;
        } else if matches!(decoded.family(), 16 | 17) {
            if !credit_catalog {
                return Err(StateTransactionError::Corrupt);
            }
            super::credit::catalog::recovery::row(tx, &table, &decoded, value.value())?;
        } else if matches!(decoded.family(), 12..=15) {
            if credit_catalog && decoded.family() == 12 {
                let scope = tx.scope_bytes(decoded.part(2))?;
                super::credit::catalog::recovery::require_meta(tx, &table, &scope)?;
            }
            let shards = credit_shards.ok_or(StateTransactionError::Corrupt)?;
            let (ids, members) =
                super::credit::recovery::row(tx, &table, &decoded, value.value(), shards)?;
            credit_counts[0] = credit_counts[0]
                .checked_add(ids)
                .ok_or(StateTransactionError::Limit)?;
            credit_counts[1] = credit_counts[1]
                .checked_add(members)
                .ok_or(StateTransactionError::Limit)?;
        } else {
            rows::validate(
                tx,
                &table,
                &decoded,
                value.value(),
                charge,
                (layout, manifest),
            )?;
        }
        if matches!(decoded.family(), 6 | 7) {
            counts[usize::from(decoded.family() - 6)] = counts[usize::from(decoded.family() - 6)]
                .checked_add(1)
                .ok_or(StateTransactionError::Limit)?;
            hash_row(&mut hash, physical.value(), value.value())?;
        }
    }
    if credit_counts[0] != credit_counts[1] {
        return Err(StateTransactionError::Corrupt);
    }
    Ok((counts[1], counts[0], hash.finalize().into()))
}
