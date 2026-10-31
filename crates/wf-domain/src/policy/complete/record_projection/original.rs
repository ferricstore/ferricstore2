//! Paid generation-free execution facts over a retained complete original donor.

use crate::policy::complete::flow_input::value::{
    PolicyInputCursor as Cursor, PolicyInputKind as Kind,
};
use crate::policy::complete::indexed_input::{borrow_attributes, borrow_metadata};
use crate::policy::complete::original_entry::{
    TermPosition,
    indexed::{OriginalPolicyCurrentIndex as Index, decode_portable_retained},
};
use crate::policy::complete::retry_input::BorrowedPolicyRetry;
use crate::policy::complete::{
    CompletePolicyEncodingError as Error, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, charge_schema,
};
use crate::policy::definition::{PolicySnapshot, StateMode, StatePolicy};
use crate::policy::restoration::{PolicySnapshotData, restore_policy_snapshot};
use crate::policy::{Backoff, BackoffKind, InstalledRetryPolicy, RetryOverride};
use crate::retention::RetentionOverride;
use std::collections::BTreeMap;

/// Measures represented mode/retry owners after a fresh full raw decode.
/// # Errors
/// Preserves profile, type binding, work and checked owner refusals.
pub fn original_record_batch_policy_projection_bytes(
    raw: &[u8],
    kind: &[u8],
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    let index = read(raw, kind, schema, charge)?;
    measure(&index, kind, charge)
}

/// Copies execution fields while the complete original bytes remain authoritative.
/// # Errors
/// Rejects the prospective owner cap before copies and preserves operational errors.
pub fn project_original_record_batch_policy(
    raw: &[u8],
    kind: &[u8],
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    maximum_owned_bytes: usize,
) -> Result<(PolicySnapshot, InstalledRetryPolicy), Error> {
    let index = read(raw, kind, schema, charge)?;
    if maximum_owned_bytes == 0 || measure(&index, kind, charge)? > maximum_owned_bytes {
        return Err(Error::ByteLimit);
    }
    let root = index.cursor().position();
    let root_retry = retry(&index, field(&index, root, b"retry", charge)?, None, charge)?;
    let mut modes = BTreeMap::new();
    let mut retries = BTreeMap::new();
    if let Some(states) =
        field(&index, root, b"states", charge)?.filter(|&p| index.at(p).kind() == Kind::Map)
    {
        for ordinal in 0..index.at(states).len() {
            pay(charge, 128)?;
            let (name, config) = index.at(states).row(ordinal).ok_or(Error::Malformed)?;
            let Some(name) = name.binary_bytes() else {
                continue;
            };
            let config = (config.kind() == Kind::Map).then_some(config.position());
            let mode = config
                .map(|p| field(&index, p, b"mode", charge))
                .transpose()?
                .flatten();
            let mode = if mode.is_some_and(|p| index.at(p).atom_bytes() == Some(b"fifo")) {
                StateMode::Fifo
            } else {
                StateMode::Parallel
            };
            let local = config
                .map(|p| field(&index, p, b"retry", charge))
                .transpose()?
                .flatten();
            let local = retry(&index, local, root_retry.backoff, charge)?;
            modes.insert(
                super::copy(name)?,
                StatePolicy {
                    mode: Some(mode),
                    retry: RetryOverride::default(),
                    retention: RetentionOverride::default(),
                },
            );
            retries.insert(super::copy(name)?, own_retry(&local)?);
        }
    }
    let mode = restore_policy_snapshot(
        PolicySnapshotData {
            workflow_type: super::copy(kind)?,
            version: None,
            max_active_ms: None,
            retry: RetryOverride::default(),
            retention: RetentionOverride::default(),
            states: modes,
        },
        schema.policy_limits(),
    )
    .map_err(|e| {
        if e == crate::policy::installation::PolicyError::StateBudget {
            Error::Schema(PolicySchemaError::Policy(e))
        } else {
            Error::UnsupportedPolicyProfile
        }
    })?;
    let retry = InstalledRetryPolicy::new(super::copy(kind)?, own_retry(&root_retry)?, retries)
        .map_err(|_| Error::UnsupportedPolicyProfile)?;
    Ok((mode, retry))
}

pub(in crate::policy::complete) fn read<'raw>(
    raw: &'raw [u8],
    kind: &[u8],
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Index<'raw, 'static>, Error> {
    let mut index = decode_portable_retained(raw, 0, schema, charge)?.ok_or(Error::Malformed)?;
    pay(
        charge,
        128 + kind.len() + index.entry().workflow_type().map_or(0, <[u8]>::len),
    )?;
    if index.entry().workflow_type() != Some(kind) {
        return Err(Error::Schema(PolicySchemaError::InvalidAuthority));
    }
    index.build_catalog(index.owner_bound()?, charge)?;
    Ok(index)
}
fn measure(
    index: &Index<'_, '_>,
    kind: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    let root = index.cursor().position();
    let root_retry = retry(index, field(index, root, b"retry", charge)?, None, charge)?;
    let mut bytes = 1024;
    super::add(&mut bytes, kind.len())?;
    super::add(&mut bytes, kind.len())?;
    super::add(&mut bytes, root_retry.exhausted_to.map_or(0, <[u8]>::len))?;
    if let Some(states) =
        field(index, root, b"states", charge)?.filter(|&p| index.at(p).kind() == Kind::Map)
    {
        for ordinal in 0..index.at(states).len() {
            pay(charge, 128)?;
            let (name, config) = index.at(states).row(ordinal).ok_or(Error::Malformed)?;
            let Some(name) = name.binary_bytes() else {
                continue;
            };
            super::add(&mut bytes, 256)?;
            super::add(&mut bytes, name.len())?;
            super::add(&mut bytes, name.len())?;
            if config.kind() == Kind::Map {
                let local = retry(
                    index,
                    field(index, config.position(), b"retry", charge)?,
                    root_retry.backoff,
                    charge,
                )?;
                super::add(&mut bytes, local.exhausted_to.map_or(0, <[u8]>::len))?;
            }
        }
    }
    Ok(bytes)
}
fn own_retry(value: &BorrowedPolicyRetry<'_>) -> Result<RetryOverride, Error> {
    Ok(RetryOverride {
        max_retries: value.max_retries,
        backoff: value.backoff,
        exhausted_to: value.exhausted_to.map(super::copy).transpose()?,
    })
}
fn retry<'raw>(
    index: &Index<'raw, '_>,
    position: Option<TermPosition>,
    inherited: Option<Backoff>,
    charge: &mut PolicySchemaCharge,
) -> Result<BorrowedPolicyRetry<'raw>, Error> {
    let mut result = BorrowedPolicyRetry {
        max_retries: None,
        backoff: None,
        exhausted_to: None,
    };
    let Some(map) = position.filter(|&p| index.at(p).kind() == Kind::Map) else {
        return Ok(result);
    };
    if let Some(p) = field(index, map, b"max_retries", charge)? {
        result.max_retries = Some(
            u16::try_from(number(index, p, 1000)?).map_err(|_| Error::UnsupportedPolicyProfile)?,
        );
    }
    if let Some(p) = field(index, map, b"exhausted_to", charge)? {
        result.exhausted_to = Some(
            index
                .at(p)
                .binary_bytes()
                .filter(|s| !s.is_empty())
                .ok_or(Error::UnsupportedPolicyProfile)?,
        );
    }
    if let Some(backoff) =
        field(index, map, b"backoff", charge)?.filter(|&p| index.at(p).kind() == Kind::Map)
    {
        let mut value = inherited.unwrap_or_default();
        if let Some(p) = field(index, backoff, b"kind", charge)? {
            value.kind = match index.at(p).atom_bytes() {
                Some(b"none") => BackoffKind::None,
                Some(b"fixed") => BackoffKind::Fixed,
                Some(b"linear") => BackoffKind::Linear,
                Some(b"exponential") => BackoffKind::Exponential,
                _ => return Err(Error::UnsupportedPolicyProfile),
            };
        }
        for (name, maximum, target) in [
            (b"base_ms".as_slice(), 2_592_000_000, &mut value.base_ms),
            (b"max_ms".as_slice(), 2_592_000_000, &mut value.max_ms),
        ] {
            if let Some(p) = field(index, backoff, name, charge)? {
                *target = number(index, p, maximum)?;
            }
        }
        if let Some(p) = field(index, backoff, b"jitter_pct", charge)? {
            value.jitter_pct = u8::try_from(number(index, p, 100)?)
                .map_err(|_| Error::UnsupportedPolicyProfile)?;
        }
        result.backoff = Some(value);
    }
    Ok(result)
}
fn number(index: &Index<'_, '_>, p: TermPosition, maximum: u64) -> Result<u64, Error> {
    index
        .at(p)
        .integer()
        .and_then(|n| u64::try_from(n).ok())
        .filter(|&n| n <= maximum)
        .ok_or(Error::UnsupportedPolicyProfile)
}
pub(in crate::policy::complete) fn field(
    index: &Index<'_, '_>,
    map: TermPosition,
    name: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<Option<TermPosition>, Error> {
    let map = index.at(map);
    let (mut low, mut high) = (0, map.len());
    while low < high {
        let middle = low + (high - low) / 2;
        pay(charge, 128)?;
        let (key, value) = map.row(middle).ok_or(Error::Malformed)?;
        let order = if let Some(bytes) = key.atom_bytes() {
            compare(bytes, name, charge)?;
            bytes.cmp(name)
        } else {
            pay(charge, 128)?;
            std::cmp::Ordering::Greater
        };
        match order {
            std::cmp::Ordering::Equal => return Ok(Some(value.position())),
            std::cmp::Ordering::Less => low = middle + 1,
            std::cmp::Ordering::Greater => high = middle,
        }
    }
    Ok(None)
}
fn compare(left: &[u8], right: &[u8], charge: &mut PolicySchemaCharge) -> Result<bool, Error> {
    pay(
        charge,
        128_usize
            .checked_add(left.len())
            .and_then(|n| n.checked_add(right.len()))
            .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?,
    )?;
    Ok(left == right)
}
fn pay(charge: &mut PolicySchemaCharge, amount: usize) -> Result<(), Error> {
    charge_schema(
        charge,
        u64::try_from(amount).map_err(|_| Error::Schema(PolicySchemaError::LengthOverflow))?,
        0,
    )
    .map_err(Error::Schema)
}

/// Compares normalized source indexed effects without changing raw authority.
/// # Errors
/// Preserves malformed/profile/work/allocation and improper-terminal refusals.
pub fn original_policy_indexed_fields_equal(
    before: Option<&[u8]>,
    after: &[u8],
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    let mut old = before
        .map(|raw| decode_portable_retained(raw, 0, schema, charge))
        .transpose()?
        .flatten();
    if before.is_some() && old.is_none() {
        return Err(Error::Malformed);
    }
    let old_owner = old.as_ref().map_or(Ok(0), Index::owner_bound)?;
    let mut new =
        decode_portable_retained(after, old_owner, schema, charge)?.ok_or(Error::Malformed)?;
    if let Some(index) = &mut old {
        let live = index
            .owner_bound()?
            .checked_add(new.owner_bound()?)
            .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
        index.build_catalog(live, charge)?;
    }
    let old_live = old.as_ref().map_or(Ok(0), |i| {
        i.owner_bound()?
            .checked_add(i.catalog_bound()?)
            .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))
    })?;
    new.build_catalog(
        old_live
            .checked_add(new.owner_bound()?)
            .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?,
        charge,
    )?;
    let old_meta = metadata(old.as_ref(), charge)?;
    let new_meta = metadata(Some(&new), charge)?;
    let (mut old_names, mut new_names) = ([None; 3], [None; 3]);
    let old_count = attributes(old.as_ref(), &mut old_names, charge)?;
    let new_count = attributes(Some(&new), &mut new_names, charge)?;
    if old_count != new_count {
        return Ok(false);
    }
    for name in old_names.into_iter().take(old_count).flatten() {
        let mut found = false;
        for candidate in new_names.into_iter().take(new_count).flatten() {
            if compare(name, candidate, charge)? {
                found = true;
                break;
            }
        }
        if !found {
            return Ok(false);
        }
    }
    match (old_meta, new_meta) {
        (None, None) => Ok(true),
        (Some(a), Some(b)) => compare(a, b, charge),
        _ => Ok(false),
    }
}
pub(super) fn metadata<'index>(
    index: Option<&'index Index<'_, 'static>>,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<&'index [u8]>, Error> {
    let Some(index) = index else {
        return Ok(None);
    };
    let Some(p) = field(
        index,
        index.cursor().position(),
        b"indexed_state_meta",
        charge,
    )?
    else {
        return Ok(None);
    };
    Ok(borrow_metadata(Cursor::Current(index.at(p)), 0, charge)?.unwrap_or(None))
}
pub(super) fn attributes<'index>(
    index: Option<&'index Index<'_, 'static>>,
    names: &mut [Option<&'index [u8]>; 3],
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    let Some(index) = index else {
        return Ok(0);
    };
    let Some(p) = field(
        index,
        index.cursor().position(),
        b"indexed_attributes",
        charge,
    )?
    else {
        return Ok(0);
    };
    let cursor = index.at(p);
    if !matches!(cursor.kind(), Kind::List | Kind::ImproperList) {
        return Ok(0);
    }
    Ok(
        borrow_attributes(Some(Cursor::Current(cursor)), names, charge)?
            .ok()
            .flatten()
            .unwrap_or(0),
    )
}
