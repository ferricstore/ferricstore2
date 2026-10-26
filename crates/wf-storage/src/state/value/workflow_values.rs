//! Borrowed aliases and scalar bindings in the closed value-capable layout.

use sha2::{Digest, Sha256};
use wf_domain::workflow::active::restore_captured_lifecycle;
use wf_domain::workflow::control::PreparedDigest;
use wf_ports::blob::{OriginalValueRefKind, WorkflowValueLifecycle};
use wf_ports::consensus::WorkflowValueDefaults;
use wf_types::version::TimestampMillis;

use super::{
    StateValueError as Error, add, allocate, finish, flag, put_raw, raw, raw_size, read_u8,
    read_u32, read_u64, read_u128, start, take,
};

const MAX_VALUE_VERSION: u64 = 9_007_199_254_740_991;

#[derive(Clone, Copy)]
pub(crate) struct ValueAlias<'a> {
    pub(crate) partition: &'a [u8],
    pub(crate) id: &'a [u8],
    pub(crate) kind: OriginalValueRefKind,
    pub(crate) version: u64,
    pub(crate) incarnation: u128,
    pub(crate) expires_at: Option<TimestampMillis>,
    pub(crate) stored_len: u64,
    pub(crate) digest: [u8; 32],
}

#[derive(Clone, Copy)]
pub(crate) struct ValueBindingData {
    pub(crate) versions: [Option<u64>; 3],
    pub(crate) lifecycle: WorkflowValueLifecycle,
}

pub(crate) fn encode_alias(alias: ValueAlias<'_>, maximum: usize) -> Result<Vec<u8>, Error> {
    validate_alias(alias)?;
    let mut size = 111;
    for field in [alias.partition, alias.id] {
        add(&mut size, raw_size(field)?)?;
    }
    let mut output = allocate(size, maximum, *b"FWVA")?;
    put_raw(&mut output, alias.partition)?;
    put_raw(&mut output, alias.id)?;
    output.push(kind_tag(alias.kind)?);
    output.extend_from_slice(&alias.version.to_be_bytes());
    output.extend_from_slice(&alias.incarnation.to_be_bytes());
    output.extend_from_slice(
        &alias
            .expires_at
            .map_or(0, TimestampMillis::get)
            .to_be_bytes(),
    );
    output.extend_from_slice(&alias.stored_len.to_be_bytes());
    output.extend_from_slice(&alias.digest);
    seal(&mut output, size)?;
    Ok(output)
}

pub(crate) fn decode_alias(bytes: &[u8]) -> Result<ValueAlias<'_>, Error> {
    let mut at = start(bytes, *b"FWVA", bytes.len())?;
    let partition = raw(bytes, &mut at)?;
    let id = raw(bytes, &mut at)?;
    let kind = match read_u8(bytes, &mut at)? {
        1 => OriginalValueRefKind::Payload,
        2 => OriginalValueRefKind::Result,
        3 => OriginalValueRefKind::Error,
        _ => return Err(Error::UnsupportedTag),
    };
    let version = read_u64(bytes, &mut at)?;
    let incarnation = read_u128(bytes, &mut at)?;
    let expiry = read_u64(bytes, &mut at)?;
    let alias = ValueAlias {
        partition,
        id,
        kind,
        version,
        incarnation,
        expires_at: (expiry != 0).then_some(TimestampMillis::new(expiry)),
        stored_len: read_u64(bytes, &mut at)?,
        digest: read_digest(bytes, &mut at)?,
    };
    verify_checksum(bytes, at)?;
    validate_alias(alias)?;
    Ok(alias)
}

pub(crate) fn encode_binding(binding: ValueBindingData, maximum: usize) -> Result<Vec<u8>, Error> {
    validate_binding(binding)?;
    let lifecycle = binding.lifecycle;
    let birth = lifecycle.birth;
    let mut size = 149;
    for value in binding.versions.into_iter().chain([
        birth.max_active_ms(),
        lifecycle.effective_ttl_ms,
        lifecycle.terminal_deadline.map(TimestampMillis::get),
    ]) {
        if value.is_some() {
            add(&mut size, 8)?;
        }
    }
    let mut output = allocate(size, maximum, *b"FWVB")?;
    for version in binding.versions {
        put_optional(&mut output, version);
    }
    output.extend_from_slice(&lifecycle.incarnation.to_be_bytes());
    put_optional(&mut output, birth.max_active_ms());
    output.extend_from_slice(&birth.retention_ttl_ms().to_be_bytes());
    output.extend_from_slice(&birth.history_hot_max_events().to_be_bytes());
    output.extend_from_slice(&birth.history_max_events().to_be_bytes());
    put_optional(&mut output, lifecycle.effective_ttl_ms);
    put_optional(
        &mut output,
        lifecycle.terminal_deadline.map(TimestampMillis::get),
    );
    output.extend_from_slice(lifecycle.configuration_digest.as_bytes());
    output.push(lifecycle.policy_profile);
    output.extend_from_slice(&lifecycle.policy_generation.to_be_bytes());
    output.extend_from_slice(lifecycle.policy_digest.as_bytes());
    seal(&mut output, size)?;
    Ok(output)
}

pub(crate) fn decode_binding(
    bytes: &[u8],
    defaults: WorkflowValueDefaults,
) -> Result<ValueBindingData, Error> {
    let mut at = start(bytes, *b"FWVB", bytes.len())?;
    let mut versions = [None; 3];
    for version in &mut versions {
        *version = read_optional(bytes, &mut at)?;
    }
    let incarnation = read_u128(bytes, &mut at)?;
    let active = read_optional(bytes, &mut at)?;
    let ttl = read_u64(bytes, &mut at)?;
    let hot = read_u32(bytes, &mut at)?;
    let history = read_u32(bytes, &mut at)?;
    let effective_ttl_ms = read_optional(bytes, &mut at)?;
    let terminal_deadline = read_optional(bytes, &mut at)?.map(TimestampMillis::new);
    let configuration_digest = PreparedDigest::new(read_digest(bytes, &mut at)?);
    let policy_profile = read_u8(bytes, &mut at)?;
    let policy_generation = read_u64(bytes, &mut at)?;
    let policy_digest = PreparedDigest::new(read_digest(bytes, &mut at)?);
    verify_checksum(bytes, at)?;
    defaults.to_bytes().map_err(Error::from)?;
    if configuration_digest != defaults.configuration_digest {
        return Err(Error::InvalidValue);
    }
    let birth = restore_captured_lifecycle(active, ttl, hot, history, defaults.lifecycle)
        .map_err(|_| Error::InvalidValue)?;
    let binding = ValueBindingData {
        versions,
        lifecycle: WorkflowValueLifecycle {
            incarnation,
            birth,
            effective_ttl_ms,
            terminal_deadline,
            configuration_digest,
            policy_profile,
            policy_generation,
            policy_digest,
        },
    };
    validate_binding(binding)?;
    Ok(binding)
}

fn validate_alias(alias: ValueAlias<'_>) -> Result<(), Error> {
    for field in [alias.partition, alias.id] {
        if field.is_empty() || field.len() > 65_535 {
            return Err(Error::InvalidValue);
        }
    }
    kind_tag(alias.kind)?;
    validate_version(alias.version)?;
    Ok(())
}

fn validate_binding(binding: ValueBindingData) -> Result<(), Error> {
    for version in binding.versions.into_iter().flatten() {
        validate_version(version)?;
    }
    let lifecycle = binding.lifecycle;
    if lifecycle.policy_profile > 2 {
        return Err(Error::UnsupportedTag);
    }
    if lifecycle.policy_profile == 0 && lifecycle.policy_generation != 0
        || lifecycle.effective_ttl_ms == Some(0)
    {
        return Err(Error::InvalidValue);
    }
    Ok(())
}

fn validate_version(version: u64) -> Result<(), Error> {
    if version == 0 || version > MAX_VALUE_VERSION {
        Err(Error::InvalidValue)
    } else {
        Ok(())
    }
}

fn kind_tag(kind: OriginalValueRefKind) -> Result<u8, Error> {
    match kind {
        OriginalValueRefKind::Payload => Ok(1),
        OriginalValueRefKind::Result => Ok(2),
        OriginalValueRefKind::Error => Ok(3),
        OriginalValueRefKind::Shared => Err(Error::UnsupportedTag),
    }
}

fn put_optional(output: &mut Vec<u8>, value: Option<u64>) {
    output.push(u8::from(value.is_some()));
    if let Some(value) = value {
        output.extend_from_slice(&value.to_be_bytes());
    }
}

fn read_optional(bytes: &[u8], at: &mut usize) -> Result<Option<u64>, Error> {
    if flag(bytes, at)? {
        Ok(Some(read_u64(bytes, at)?))
    } else {
        Ok(None)
    }
}

fn read_digest(bytes: &[u8], at: &mut usize) -> Result<[u8; 32], Error> {
    take(bytes, at, 32)?
        .try_into()
        .map_err(|_| Error::Malformed)
}

fn seal(output: &mut Vec<u8>, size: usize) -> Result<(), Error> {
    if output.len().checked_add(32) != Some(size) {
        return Err(Error::Malformed);
    }
    let checksum = Sha256::digest(output.as_slice());
    output.extend_from_slice(&checksum);
    Ok(())
}

fn verify_checksum(bytes: &[u8], mut at: usize) -> Result<(), Error> {
    let hash_at = at;
    let checksum = take(bytes, &mut at, 32)?;
    finish(bytes, at)?;
    if checksum != Sha256::digest(&bytes[..hash_at]).as_slice() {
        return Err(Error::InvalidValue);
    }
    Ok(())
}
