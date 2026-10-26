//! Closed birth/current manifest and two complete actual global catalog passes.

use super::super::{
    admission::{copy_bytes, engine_pair, storage_error, table_error, total, value_error},
    bootstrap::hash_row,
    recovery,
    store::RedbStateTransaction,
};
use crate::state::{key, value};
use redb::ReadableTable;
use sha2::{Digest, Sha256};
use wf_ports::state::StateTransactionError as Error;
use wf_types::version::{CommittedTime, HlcTimestamp};

pub(in crate::state::redb) const POLICY_LAYOUT: u16 = 5;
const ORIGIN_KEY: &[u8] = b"origin";
pub(in crate::state::redb) const CURRENT_CATALOG_DOMAIN: &[u8] = b"wf-state-current-catalog-v1";

#[derive(Clone, Copy)]
pub(in crate::state::redb) struct CurrentPolicyManifest {
    pub(in crate::state::redb) initial_time: CommittedTime,
    pub(in crate::state::redb) placements: u64,
    pub(in crate::state::redb) policies: u64,
    pub(in crate::state::redb) catalog_hash: [u8; 32],
    pub(in crate::state::redb) origin_hash: [u8; 32],
}

pub(in crate::state::redb) fn read(
    tx: &RedbStateTransaction<'_>,
) -> Result<(u16, CurrentPolicyManifest), Error> {
    tx.step()?;
    let table = tx
        .write
        .open_table(tx.metadata_table)
        .map_err(table_error)?;
    tx.step()?;
    let bytes = table
        .get(b"manifest".as_slice())
        .map_err(|e| storage_error(&e))?
        .ok_or(Error::Corrupt)?;
    let version = version(bytes.value())?;
    let facts = if matches!(version, 5..=7) {
        parse5(tx, bytes.value())?
    } else {
        let mut layout = 0;
        let (initial_time, placements, policies, catalog_hash) =
            recovery::check_manifest(bytes.value(), tx, &mut layout)?;
        if !matches!(layout, 3 | 4) {
            return Err(Error::UnsupportedLayout);
        }
        CurrentPolicyManifest {
            initial_time,
            placements,
            policies,
            catalog_hash,
            origin_hash: [0; 32],
        }
    };
    tx.step()?;
    let origin = table.get(ORIGIN_KEY).map_err(|e| storage_error(&e))?;
    if matches!(version, 5..=7) {
        bind_origin(tx, facts, origin.ok_or(Error::Corrupt)?.value())?;
    } else if origin.is_some() {
        return Err(Error::Corrupt);
    }
    Ok((version, facts))
}
pub(in crate::state::redb) fn version(bytes: &[u8]) -> Result<u16, Error> {
    if bytes.get(..4) != Some(b"FWSS") {
        return Err(Error::Corrupt);
    }
    Ok(u16::from_be_bytes(
        bytes
            .get(4..6)
            .ok_or(Error::Corrupt)?
            .try_into()
            .map_err(|_| Error::Corrupt)?,
    ))
}
pub(in crate::state::redb) fn parse5(
    tx: &RedbStateTransaction<'_>,
    bytes: &[u8],
) -> Result<CurrentPolicyManifest, Error> {
    let layout = version(bytes)?;
    if layout == 7 {
        return Ok(super::super::values::manifest::parse(bytes, tx.group, tx.recovery)?.policy);
    }
    if !matches!(layout, 5 | 6) {
        return Err(Error::Corrupt);
    }
    let mut at = 6;
    for expected in [tx.group.as_bytes(), tx.recovery.as_bytes()] {
        let n = usize::from(value::read_u16(bytes, &mut at).map_err(value_error)?);
        if n == 0 || n > 256 || value::take(bytes, &mut at, n).map_err(value_error)? != expected {
            return Err(Error::Corrupt);
        }
    }
    let physical = value::read_u64(bytes, &mut at).map_err(value_error)?;
    let logical = value::read_u16(bytes, &mut at).map_err(value_error)?;
    let wall = value::read_u64(bytes, &mut at).map_err(value_error)?;
    let initial_time = CommittedTime::new(
        HlcTimestamp::new(physical, logical).map_err(|_| Error::Corrupt)?,
        wall,
    )
    .map_err(|_| Error::Corrupt)?;
    let placements = value::read_u64(bytes, &mut at).map_err(value_error)?;
    let policies = value::read_u64(bytes, &mut at).map_err(value_error)?;
    let catalog_hash = value::take(bytes, &mut at, 32)
        .map_err(value_error)?
        .try_into()
        .map_err(|_| Error::Corrupt)?;
    let origin_hash = value::take(bytes, &mut at, 32)
        .map_err(value_error)?
        .try_into()
        .map_err(|_| Error::Corrupt)?;
    let profile = if layout == 6 {
        Some(value::read_u32(bytes, &mut at).map_err(value_error)?)
    } else {
        None
    };
    let checksum = Sha256::digest(&bytes[..at]);
    if value::take(bytes, &mut at, 32).map_err(value_error)? != checksum.as_slice() {
        return Err(Error::Corrupt);
    }
    value::finish(bytes, at).map_err(value_error)?;
    if profile.is_some_and(|id| id != 1) {
        return Err(Error::UnsupportedPolicyProfile);
    }
    Ok(CurrentPolicyManifest {
        initial_time,
        placements,
        policies,
        catalog_hash,
        origin_hash,
    })
}
pub(in crate::state::redb) fn facts(
    manifest: CurrentPolicyManifest,
) -> (CommittedTime, u64, u64, [u8; 32]) {
    (
        manifest.initial_time,
        manifest.placements,
        manifest.policies,
        manifest.catalog_hash,
    )
}
pub(in crate::state::redb) fn bind_origin(
    tx: &RedbStateTransaction<'_>,
    manifest: CurrentPolicyManifest,
    origin: &[u8],
) -> Result<(), Error> {
    let mut version = 0;
    let (birth, _, _, _) = recovery::check_manifest(origin, tx, &mut version)?;
    if version != 3
        || birth != manifest.initial_time
        || Sha256::digest(origin).as_slice() != manifest.origin_hash
    {
        return Err(Error::Corrupt);
    }
    Ok(())
}
pub(in crate::state::redb) fn verify(
    tx: &RedbStateTransaction<'_>,
    layout: u16,
    manifest: CurrentPolicyManifest,
) -> Result<(), Error> {
    let actual = census(tx, layout)?;
    if actual
        != (
            manifest.placements,
            manifest.policies,
            manifest.catalog_hash,
        )
    {
        return Err(Error::Corrupt);
    }
    Ok(())
}
pub(in crate::state::redb) fn census(
    tx: &RedbStateTransaction<'_>,
    layout: u16,
) -> Result<(u64, u64, [u8; 32]), Error> {
    let mut hash = Sha256::new();
    hash.update(if matches!(layout, 6 | 7) {
        super::super::snapshot::original::CATALOG_DOMAIN
    } else if layout == POLICY_LAYOUT {
        CURRENT_CATALOG_DOMAIN
    } else {
        b"wf-state-genesis-v1"
    });
    let mut counts = [0_u64; 2];
    for family in [6_u8, 7] {
        let prefix = tx.key(&[], 0, |cap| key::encode_catalog_prefix(family, cap))?;
        tx.temporary(prefix.len())?;
        tx.step()?;
        let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
        tx.step()?;
        let mut rows = table
            .range(prefix.as_slice()..)
            .map_err(|e| storage_error(&e))?;
        loop {
            tx.step()?;
            let Some((k, v)) = rows.next().transpose().map_err(|e| storage_error(&e))? else {
                break;
            };
            if !k.value().starts_with(&prefix) {
                break;
            }
            tx.recovery_row(k.value(), v.value())?;
            tx.temporary(total(k.value().len(), [v.value().len()])?)?;
            let maximum = total(256, [k.value().len()])?;
            tx.admit_read(maximum)?;
            tx.temporary(maximum)?;
            let decoded =
                key::decode_state_key(k.value(), tx.group, tx.recovery, k.value().len(), maximum)
                    .map_err(value_error)?;
            if decoded.family() != family {
                return Err(Error::Corrupt);
            }
            counts[usize::from(family - 6)] = counts[usize::from(family - 6)]
                .checked_add(1)
                .ok_or(Error::Limit)?;
            hash_row(&mut hash, k.value(), v.value())?;
        }
    }
    Ok((counts[1], counts[0], hash.finalize().into()))
}

pub(in crate::state::redb) fn publish(
    tx: &RedbStateTransaction<'_>,
    old_layout: u16,
    manifest: CurrentPolicyManifest,
    catalog: &super::super::snapshot::catalog::SnapshotCatalog,
) -> Result<(), Error> {
    publish_target(
        tx,
        old_layout,
        manifest,
        catalog,
        old_layout.max(POLICY_LAYOUT),
    )
}

pub(in crate::state::redb) fn publish_original(
    tx: &RedbStateTransaction<'_>,
    old_layout: u16,
    manifest: CurrentPolicyManifest,
    catalog: &super::super::snapshot::catalog::SnapshotCatalog,
) -> Result<(), Error> {
    publish_target(
        tx,
        old_layout,
        manifest,
        catalog,
        if old_layout == 7 { 7 } else { 6 },
    )
}

fn publish_target(
    tx: &RedbStateTransaction<'_>,
    old_layout: u16,
    mut manifest: CurrentPolicyManifest,
    catalog: &super::super::snapshot::catalog::SnapshotCatalog,
    target: u16,
) -> Result<(), Error> {
    if old_layout < POLICY_LAYOUT {
        tx.step()?;
        let mut table = tx
            .write
            .open_table(tx.metadata_table)
            .map_err(table_error)?;
        tx.step()?;
        let original = table
            .get(b"manifest".as_slice())
            .map_err(|e| storage_error(&e))?
            .ok_or(Error::Corrupt)?;
        tx.temporary(original.value().len())?;
        let mut origin = copy_bytes(original.value())?;
        origin[4..6].copy_from_slice(&3_u16.to_be_bytes());
        let end = origin.len().checked_sub(32).ok_or(Error::Corrupt)?;
        let checksum = Sha256::digest(&origin[..end]);
        origin[end..].copy_from_slice(&checksum);
        manifest.origin_hash = Sha256::digest(&origin).into();
        drop(original);
        engine_pair(ORIGIN_KEY.len(), origin.len())?;
        tx.step()?;
        table
            .insert(ORIGIN_KEY, origin.as_slice())
            .map_err(|e| storage_error(&e))?;
        drop(table);
        super::super::snapshot::policy::upgrade_catalog(tx, catalog)?;
    }
    let (placements, policies, hash) = census(tx, target)?;
    manifest.placements = placements;
    manifest.policies = policies;
    manifest.catalog_hash = hash;
    if target == 7 {
        let defaults = super::super::values::manifest::read(tx)?.defaults;
        return super::super::values::manifest::publish(
            tx,
            super::super::values::manifest::ValueManifest {
                policy: manifest,
                defaults,
            },
        );
    }
    let n = total(
        if target == 6 { 144 } else { 140 },
        [tx.group.as_bytes().len(), tx.recovery.as_bytes().len()],
    )?;
    tx.temporary(n)?;
    engine_pair(8, n)?;
    let mut output = Vec::new();
    output.try_reserve_exact(n).map_err(|_| Error::Allocation)?;
    output.extend_from_slice(b"FWSS");
    output.extend_from_slice(&target.to_be_bytes());
    for part in [tx.group.as_bytes(), tx.recovery.as_bytes()] {
        output.extend_from_slice(
            &u16::try_from(part.len())
                .map_err(|_| Error::Limit)?
                .to_be_bytes(),
        );
        output.extend_from_slice(part);
    }
    output.extend_from_slice(&manifest.initial_time.hlc().physical().get().to_be_bytes());
    output.extend_from_slice(&manifest.initial_time.hlc().logical().to_be_bytes());
    output.extend_from_slice(&manifest.initial_time.wall().get().to_be_bytes());
    for count in [placements, policies] {
        output.extend_from_slice(&count.to_be_bytes());
    }
    output.extend_from_slice(&hash);
    output.extend_from_slice(&manifest.origin_hash);
    if target == 6 {
        output.extend_from_slice(&1_u32.to_be_bytes());
    }
    output.extend_from_slice(&Sha256::digest(&output));
    tx.step()?;
    let mut table = tx
        .write
        .open_table(tx.metadata_table)
        .map_err(table_error)?;
    tx.step()?;
    table
        .insert(b"manifest".as_slice(), output.as_slice())
        .map_err(|e| storage_error(&e))?;
    Ok(())
}
