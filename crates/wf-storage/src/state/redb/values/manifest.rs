//! Exact value-capable current manifest with immutable configured defaults.

use redb::ReadableTable;
use sha2::{Digest, Sha256};
use wf_ports::consensus::WorkflowValueDefaults;
use wf_ports::state::StateTransactionError as Error;
use wf_types::identity::{GroupId, RecoveryId};
use wf_types::version::{CommittedTime, HlcTimestamp};

use super::super::{
    admission::{storage_error, table_error, total, value_error},
    core_policy::catalog::CurrentPolicyManifest,
    store::RedbStateTransaction,
};
use crate::state::value;

#[derive(Clone, Copy)]
pub(crate) struct ValueManifest {
    pub(crate) layout: u16,
    pub(in crate::state::redb) policy: CurrentPolicyManifest,
    pub(crate) defaults: WorkflowValueDefaults,
    pub(crate) alias_format: u16,
    pub(crate) credit_shard_count: Option<u64>,
}

pub(crate) fn read(tx: &RedbStateTransaction<'_>) -> Result<ValueManifest, Error> {
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
    tx.recovery_row(b"manifest", bytes.value())?;
    parse(bytes.value(), tx.group, tx.recovery)
}

pub(crate) fn parse(
    bytes: &[u8],
    group: &GroupId,
    recovery: &RecoveryId,
) -> Result<ValueManifest, Error> {
    let mut at = 0;
    if value::take(bytes, &mut at, 4).map_err(value_error)? != b"FWSS" {
        return Err(Error::Corrupt);
    }
    let layout = value::read_u16(bytes, &mut at).map_err(value_error)?;
    if !matches!(layout, 7 | 8) {
        return Err(Error::Corrupt);
    }
    for expected in [group.as_bytes(), recovery.as_bytes()] {
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
    let alias_format = match value::read_u32(bytes, &mut at).map_err(value_error)? {
        1 => 1,
        2 => 2,
        _ => return Err(Error::UnsupportedPolicyProfile),
    };
    if value::read_u32(bytes, &mut at).map_err(value_error)? != 1 {
        return Err(Error::UnsupportedPolicyProfile);
    }
    let raw = value::take(bytes, &mut at, 60)
        .map_err(value_error)?
        .try_into()
        .map_err(|_| Error::Corrupt)?;
    let defaults = WorkflowValueDefaults::from_bytes(raw).map_err(|_| Error::Corrupt)?;
    let credit_shard_count = match bytes.len().checked_sub(at) {
        Some(32) => None,
        Some(42) if layout == 8 && alias_format == 2 => {
            if value::read_u16(bytes, &mut at).map_err(value_error)? != 1 {
                return Err(Error::UnsupportedPolicyProfile);
            }
            let count = value::read_u64(bytes, &mut at).map_err(value_error)?;
            if count == 0 || count > wf_domain::workflow::MAX_EXACT_MILLIS {
                return Err(Error::Corrupt);
            }
            Some(count)
        }
        _ => return Err(Error::Corrupt),
    };
    let checksum = Sha256::digest(&bytes[..at]);
    if value::take(bytes, &mut at, 32).map_err(value_error)? != checksum.as_slice() {
        return Err(Error::Corrupt);
    }
    value::finish(bytes, at).map_err(value_error)?;
    Ok(ValueManifest {
        layout,
        policy: CurrentPolicyManifest {
            initial_time,
            placements,
            policies,
            catalog_hash,
            origin_hash,
        },
        defaults,
        alias_format,
        credit_shard_count,
    })
}

pub(crate) fn encode(
    manifest: ValueManifest,
    group: &GroupId,
    recovery: &RecoveryId,
    maximum: usize,
) -> Result<Vec<u8>, Error> {
    let size = encoded_size(manifest, group, recovery)?;
    if size > maximum {
        return Err(Error::Limit);
    }
    if !matches!(manifest.alias_format, 1 | 2) || !matches!(manifest.layout, 7 | 8) {
        return Err(Error::UnsupportedPolicyProfile);
    }
    let defaults = manifest
        .defaults
        .to_bytes()
        .map_err(|_| Error::InvalidChanges)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| Error::Allocation)?;
    bytes.extend_from_slice(b"FWSS");
    bytes.extend_from_slice(&manifest.layout.to_be_bytes());
    for part in [group.as_bytes(), recovery.as_bytes()] {
        bytes.extend_from_slice(
            &u16::try_from(part.len())
                .map_err(|_| Error::Limit)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(part);
    }
    let policy = manifest.policy;
    bytes.extend_from_slice(&policy.initial_time.hlc().physical().get().to_be_bytes());
    bytes.extend_from_slice(&policy.initial_time.hlc().logical().to_be_bytes());
    bytes.extend_from_slice(&policy.initial_time.wall().get().to_be_bytes());
    bytes.extend_from_slice(&policy.placements.to_be_bytes());
    bytes.extend_from_slice(&policy.policies.to_be_bytes());
    bytes.extend_from_slice(&policy.catalog_hash);
    bytes.extend_from_slice(&policy.origin_hash);
    bytes.extend_from_slice(&u32::from(manifest.alias_format).to_be_bytes());
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&defaults);
    if let Some(count) = manifest.credit_shard_count {
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.extend_from_slice(&count.to_be_bytes());
    }
    let checksum = Sha256::digest(&bytes);
    bytes.extend_from_slice(&checksum);
    if bytes.len() != size {
        return Err(Error::Corrupt);
    }
    Ok(bytes)
}

pub(crate) fn publish(tx: &RedbStateTransaction<'_>, manifest: ValueManifest) -> Result<(), Error> {
    let size = encoded_size(manifest, tx.group, tx.recovery)?;
    tx.temporary(size)?;
    let bytes = encode(manifest, tx.group, tx.recovery, size)?;
    super::super::admission::engine_pair(8, bytes.len())?;
    tx.step()?;
    let mut table = tx
        .write
        .open_table(tx.metadata_table)
        .map_err(table_error)?;
    tx.step()?;
    table
        .insert(b"manifest".as_slice(), bytes.as_slice())
        .map_err(|e| storage_error(&e))?;
    Ok(())
}

fn encoded_size(
    manifest: ValueManifest,
    group: &GroupId,
    recovery: &RecoveryId,
) -> Result<usize, Error> {
    if manifest.credit_shard_count.is_some_and(|count| {
        count == 0
            || count > wf_domain::workflow::MAX_EXACT_MILLIS
            || manifest.layout != 8
            || manifest.alias_format != 2
    }) {
        return Err(Error::InvalidChanges);
    }
    total(
        208,
        [
            group.as_bytes().len(),
            recovery.as_bytes().len(),
            if manifest.credit_shard_count.is_some() {
                10
            } else {
                0
            },
        ],
    )
}
