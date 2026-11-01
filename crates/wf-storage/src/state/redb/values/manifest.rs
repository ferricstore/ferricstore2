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

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum GovernanceCatalogProfile {
    Disabled,
    Membership,
    Progress,
    Prune,
    WorkflowRelease,
    GovernedClaim,
}
impl GovernanceCatalogProfile {
    pub(crate) const fn enabled(self) -> bool {
        !matches!(self, Self::Disabled)
    }
    pub(crate) const fn progress(self) -> bool {
        matches!(
            self,
            Self::Progress | Self::Prune | Self::WorkflowRelease | Self::GovernedClaim
        )
    }
    pub(crate) const fn prune(self) -> bool {
        matches!(
            self,
            Self::Prune | Self::WorkflowRelease | Self::GovernedClaim
        )
    }
    pub(crate) const fn workflow_release(self) -> bool {
        matches!(self, Self::WorkflowRelease | Self::GovernedClaim)
    }
    pub(crate) const fn version(self) -> u16 {
        match self {
            Self::Disabled => 0,
            Self::Membership => 1,
            Self::Progress => 2,
            Self::Prune => 3,
            Self::WorkflowRelease => 4,
            Self::GovernedClaim => 5,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ValueManifest {
    pub(crate) binding_format: u16,
    pub(crate) layout: u16,
    pub(in crate::state::redb) policy: CurrentPolicyManifest,
    pub(crate) defaults: WorkflowValueDefaults,
    pub(crate) alias_format: u16,
    pub(crate) credit_shard_count: Option<u64>,
    pub(crate) credit_catalog: bool,
    pub(crate) governance_catalog: GovernanceCatalogProfile,
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
    let binding_format = read_binding_format(bytes, &mut at, layout, alias_format)?;
    let raw = value::take(bytes, &mut at, 60)
        .map_err(value_error)?
        .try_into()
        .map_err(|_| Error::Corrupt)?;
    let defaults = WorkflowValueDefaults::from_bytes(raw).map_err(|_| Error::Corrupt)?;
    let (credit_shard_count, credit_catalog) = match bytes.len().checked_sub(at) {
        Some(32 | 36) => (None, false),
        Some(42 | 46) if layout == 8 && alias_format == 2 => {
            let catalog = match value::read_u16(bytes, &mut at).map_err(value_error)? {
                1 => false,
                2 => true,
                _ => return Err(Error::UnsupportedPolicyProfile),
            };
            let count = value::read_u64(bytes, &mut at).map_err(value_error)?;
            if count == 0 || count > wf_domain::workflow::MAX_EXACT_MILLIS {
                return Err(Error::Corrupt);
            }
            (Some(count), catalog)
        }
        _ => return Err(Error::Corrupt),
    };
    let governance_catalog = if bytes.len().checked_sub(at) == Some(36) {
        if layout != 8 || alias_format != 2 {
            return Err(Error::Corrupt);
        }
        match value::take(bytes, &mut at, 4).map_err(value_error)? {
            b"GC\0\x01" => GovernanceCatalogProfile::Membership,
            b"GC\0\x02" => GovernanceCatalogProfile::Progress,
            b"GC\0\x03" => GovernanceCatalogProfile::Prune,
            b"GC\0\x04" => GovernanceCatalogProfile::WorkflowRelease,
            b"GC\0\x05" => GovernanceCatalogProfile::GovernedClaim,
            _ => return Err(Error::Corrupt),
        }
    } else {
        GovernanceCatalogProfile::Disabled
    };
    if governance_catalog.workflow_release() && (!credit_catalog || credit_shard_count.is_none()) {
        return Err(Error::Corrupt);
    }
    let checksum = Sha256::digest(&bytes[..at]);
    if value::take(bytes, &mut at, 32).map_err(value_error)? != checksum.as_slice() {
        return Err(Error::Corrupt);
    }
    value::finish(bytes, at).map_err(value_error)?;
    Ok(ValueManifest {
        binding_format,
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
        credit_catalog,
        governance_catalog,
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
    if !matches!(manifest.binding_format, 1 | 2)
        || (manifest.binding_format == 2 && (manifest.layout != 8 || manifest.alias_format != 2))
    {
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
    bytes.extend_from_slice(&u32::from(manifest.binding_format).to_be_bytes());
    bytes.extend_from_slice(&defaults);
    if let Some(count) = manifest.credit_shard_count {
        bytes.extend_from_slice(&(if manifest.credit_catalog { 2_u16 } else { 1 }).to_be_bytes());
        bytes.extend_from_slice(&count.to_be_bytes());
    }
    if manifest.governance_catalog.enabled() {
        bytes.extend_from_slice(
            if manifest.governance_catalog == GovernanceCatalogProfile::GovernedClaim {
                b"GC\0\x05"
            } else if manifest.governance_catalog.workflow_release() {
                b"GC\0\x04"
            } else if manifest.governance_catalog.prune() {
                b"GC\0\x03"
            } else if manifest.governance_catalog.progress() {
                b"GC\0\x02"
            } else {
                b"GC\0\x01"
            },
        );
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
    if manifest.credit_catalog && manifest.credit_shard_count.is_none() {
        return Err(Error::InvalidChanges);
    }
    if manifest.governance_catalog.workflow_release()
        && (!manifest.credit_catalog || manifest.credit_shard_count.is_none())
    {
        return Err(Error::InvalidChanges);
    }
    if manifest.governance_catalog.enabled() && (manifest.layout != 8 || manifest.alias_format != 2)
    {
        return Err(Error::InvalidChanges);
    }
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
            if manifest.governance_catalog.enabled() {
                4
            } else {
                0
            },
        ],
    )
}

fn read_binding_format(
    bytes: &[u8],
    at: &mut usize,
    layout: u16,
    alias: u16,
) -> Result<u16, Error> {
    match value::read_u32(bytes, at).map_err(value_error)? {
        1 => Ok(1),
        2 if layout == 8 && alias == 2 => Ok(2),
        _ => Err(Error::UnsupportedPolicyProfile),
    }
}
