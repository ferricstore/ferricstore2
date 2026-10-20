//! Canonical framed emission and scope-bound descriptor bytes.

use super::super::store::RedbStateTransaction;
use super::{KEY, SnapshotError, SnapshotLimits, admission, io};
use crate::state::value;
use redb::{ReadableTable, TableHandle};
use sha2::{Digest, Sha256};
use wf_ports::state::StateTransactionError as Error;

fn sizes(
    tx: &RedbStateTransaction<'_>,
    limits: SnapshotLimits,
) -> Result<(usize, usize, u16), Error> {
    let mut rows = 0_usize;
    let mut version = 1_u16;
    let mut wire = admission::total(
        66,
        [tx.group.as_bytes().len(), tx.recovery.as_bytes().len()],
    )?;
    for definition in [tx.metadata_table, tx.rows_table] {
        tx.step()?;
        let table = tx
            .write
            .open_table(definition)
            .map_err(admission::table_error)?;
        tx.step()?;
        let mut entries = table.iter().map_err(|e| admission::storage_error(&e))?;
        loop {
            tx.step()?;
            let Some(row) = entries.next() else {
                break;
            };
            let (key, value) = row.map_err(|e| admission::storage_error(&e))?;
            tx.recovery_row(key.value(), value.value())?;
            if !tx.snapshot_cohort
                && definition.name() == super::super::META.name()
                && key.value() == KEY
            {
                continue;
            }
            if key.value() == b"manifest" && value.value().get(4..6) == Some([0, 5].as_slice()) {
                version = 2;
            }
            pair(key.value().len(), value.value().len(), limits)?;
            rows = admission::total(rows, [1])?;
            wire = admission::total(wire, [12, key.value().len(), value.value().len()])?;
            if rows > limits.maximum_rows() || wire > limits.maximum_encoded_bytes() {
                return Err(Error::Limit);
            }
        }
    }
    Ok((rows, wire, version))
}

pub(super) fn pair(key: usize, value: usize, limits: SnapshotLimits) -> Result<(), Error> {
    admission::engine_pair(key, value)?;
    if admission::total(key, [value])? > limits.maximum_row_bytes() {
        return Err(Error::Limit);
    }
    u32::try_from(key).map_err(|_| Error::Limit)?;
    u64::try_from(value).map_err(|_| Error::Limit)?;
    Ok(())
}

fn push<W: std::io::Write>(
    tx: &RedbStateTransaction<'_>,
    output: &mut Option<&mut W>,
    limits: SnapshotLimits,
    hash: &mut Sha256,
    bytes: &[u8],
) -> Result<(), SnapshotError> {
    if let Some(output) = output {
        io::write(tx, *output, bytes, limits)?;
    }
    hash.update(bytes);
    Ok(())
}

fn header<W: std::io::Write>(
    tx: &RedbStateTransaction<'_>,
    output: &mut Option<&mut W>,
    limits: SnapshotLimits,
    hash: &mut Sha256,
    counts: (usize, usize, u16),
) -> Result<(), SnapshotError> {
    push(
        tx,
        output,
        limits,
        hash,
        if counts.2 == 2 {
            b"FWSP\0\x02"
        } else {
            b"FWSP\0\x01"
        },
    )?;
    for part in [tx.group.as_bytes(), tx.recovery.as_bytes()] {
        let n = u16::try_from(part.len()).map_err(|_| SnapshotError::Limit)?;
        push(tx, output, limits, hash, &n.to_be_bytes())?;
        push(tx, output, limits, hash, part)?;
    }
    let meta = if counts.2 == 2 { 3 } else { 2 };
    let rows = counts.0.checked_sub(meta).ok_or(SnapshotError::Malformed)?;
    for n in [meta, rows, counts.1] {
        let n = u64::try_from(n).map_err(|_| SnapshotError::Limit)?;
        push(tx, output, limits, hash, &n.to_be_bytes())?;
    }
    Ok(())
}

fn entries<W: std::io::Write>(
    tx: &RedbStateTransaction<'_>,
    output: &mut Option<&mut W>,
    limits: SnapshotLimits,
    hash: &mut Sha256,
) -> Result<(), SnapshotError> {
    for definition in [tx.metadata_table, tx.rows_table] {
        tx.step().map_err(SnapshotError::State)?;
        let table = tx
            .write
            .open_table(definition)
            .map_err(admission::table_error)
            .map_err(SnapshotError::State)?;
        tx.step().map_err(SnapshotError::State)?;
        let mut rows = table
            .iter()
            .map_err(|e| SnapshotError::State(admission::storage_error(&e)))?;
        loop {
            tx.step().map_err(SnapshotError::State)?;
            let Some(row) = rows.next() else {
                break;
            };
            let (key, value) =
                row.map_err(|e| SnapshotError::State(admission::storage_error(&e)))?;
            tx.recovery_row(key.value(), value.value())
                .map_err(SnapshotError::State)?;
            if !tx.snapshot_cohort
                && definition.name() == super::super::META.name()
                && key.value() == KEY
            {
                continue;
            }
            let k = u32::try_from(key.value().len()).map_err(|_| SnapshotError::Limit)?;
            let v = u64::try_from(value.value().len()).map_err(|_| SnapshotError::Limit)?;
            push(tx, output, limits, hash, &k.to_be_bytes())?;
            push(tx, output, limits, hash, &v.to_be_bytes())?;
            push(tx, output, limits, hash, key.value())?;
            let normalized = if !tx.snapshot_cohort
                && key.value() == b"manifest"
                && value.value().get(4..6) == Some([0, 4].as_slice())
            {
                Some(manifest(tx, value.value(), 3).map_err(SnapshotError::State)?)
            } else {
                None
            };
            push(
                tx,
                output,
                limits,
                hash,
                normalized.as_deref().unwrap_or(value.value()),
            )?;
        }
    }
    Ok(())
}

pub(super) fn same_genesis(old: &[u8], new: &[u8]) -> bool {
    old.len() == new.len()
        && old.len() >= 38
        && old[..4] == *b"FWSS"
        && new[..4] == *b"FWSS"
        && old[6..old.len() - 32] == new[6..new.len() - 32]
}

pub(super) fn manifest(
    tx: &RedbStateTransaction<'_>,
    bytes: &[u8],
    version: u16,
) -> Result<Vec<u8>, Error> {
    if bytes.len() < 38 || &bytes[..4] != b"FWSS" {
        return Err(Error::Corrupt);
    }
    tx.temporary(bytes.len())?;
    let mut result = admission::copy_bytes(bytes)?;
    result[4..6].copy_from_slice(&version.to_be_bytes());
    let at = result.len() - 32;
    let hash = Sha256::digest(&result[..at]);
    result[at..].copy_from_slice(&hash);
    Ok(result)
}

pub(super) fn set_manifest(tx: &RedbStateTransaction<'_>, version: u16) -> Result<(), Error> {
    tx.step()?;
    let mut table = tx
        .write
        .open_table(super::super::META)
        .map_err(admission::table_error)?;
    let bytes = {
        tx.step()?;
        let value = table
            .get(b"manifest".as_slice())
            .map_err(|e| admission::storage_error(&e))?
            .ok_or(Error::Corrupt)?;
        tx.recovery_row(b"manifest", value.value())?;
        manifest(tx, value.value(), version)?
    };
    tx.step()?;
    tx.recovery_row(b"manifest", &bytes)?;
    table
        .insert(b"manifest".as_slice(), bytes.as_slice())
        .map_err(|e| admission::storage_error(&e))?;
    Ok(())
}

pub(super) fn emit<W: std::io::Write>(
    tx: &RedbStateTransaction<'_>,
    mut output: Option<&mut W>,
    limits: SnapshotLimits,
) -> Result<([u8; 32], usize, usize), SnapshotError> {
    let counts = sizes(tx, limits).map_err(SnapshotError::State)?;
    let mut hash = Sha256::new();
    header(tx, &mut output, limits, &mut hash, counts)?;
    entries(tx, &mut output, limits, &mut hash)?;
    let footer = hash.clone().finalize();
    push(tx, &mut output, limits, &mut hash, &footer)?;
    Ok((hash.finalize().into(), counts.0, counts.1))
}

pub(super) fn facts(
    tx: &RedbStateTransaction<'_>,
    limits: SnapshotLimits,
) -> Result<([u8; 32], usize, usize), Error> {
    emit::<std::io::Sink>(tx, None, limits).map_err(|error| match error {
        SnapshotError::State(error) => error,
        SnapshotError::Allocation => Error::Allocation,
        SnapshotError::Limit => Error::Limit,
        _ => Error::Corrupt,
    })
}

fn descriptor_hash(tx: &RedbStateTransaction<'_>, prefix: &[u8]) -> Result<[u8; 32], Error> {
    let mut hash = Sha256::new();
    hash.update(b"wf-state-snapshot-descriptor-v1");
    for part in [tx.group.as_bytes(), tx.recovery.as_bytes()] {
        hash.update(
            u16::try_from(part.len())
                .map_err(|_| Error::Limit)?
                .to_be_bytes(),
        );
        hash.update(part);
    }
    hash.update(prefix);
    Ok(hash.finalize().into())
}

pub(super) fn descriptor(
    tx: &RedbStateTransaction<'_>,
    facts: Option<([u8; 32], usize, usize)>,
    pinned: bool,
    out: &mut [u8; 87],
) -> Result<usize, Error> {
    if pinned && facts.is_none() {
        return Err(Error::Corrupt);
    }
    out[..6].copy_from_slice(if pinned { b"FWSD\0\x02" } else { b"FWSD\0\x01" });
    out[6] = u8::from(facts.is_some());
    let at = if let Some((id, rows, bytes)) = facts {
        out[7..15].copy_from_slice(&u64::try_from(rows).map_err(|_| Error::Limit)?.to_be_bytes());
        out[15..23].copy_from_slice(
            &u64::try_from(bytes)
                .map_err(|_| Error::Limit)?
                .to_be_bytes(),
        );
        out[23..55].copy_from_slice(&id);
        55
    } else {
        7
    };
    let hash = descriptor_hash(tx, &out[..at])?;
    out[at..at + 32].copy_from_slice(&hash);
    Ok(at + 32)
}

pub(super) fn decode_descriptor(
    tx: &RedbStateTransaction<'_>,
    bytes: &[u8],
) -> Result<Option<([u8; 32], usize, usize)>, Error> {
    let mut at = 0;
    let header = value::take(bytes, &mut at, 6).map_err(admission::value_error)?;
    if header != b"FWSD\0\x01" && header != b"FWSD\0\x02" {
        return Err(Error::Corrupt);
    }
    let result = match value::read_u8(bytes, &mut at).map_err(admission::value_error)? {
        0 if header == b"FWSD\0\x01" => None,
        1 => {
            let rows =
                usize::try_from(value::read_u64(bytes, &mut at).map_err(admission::value_error)?)
                    .map_err(|_| Error::Limit)?;
            let wire =
                usize::try_from(value::read_u64(bytes, &mut at).map_err(admission::value_error)?)
                    .map_err(|_| Error::Limit)?;
            let id = value::take(bytes, &mut at, 32)
                .map_err(admission::value_error)?
                .try_into()
                .map_err(|_| Error::Corrupt)?;
            if rows < 3 || wire == 0 {
                return Err(Error::Corrupt);
            }
            Some((id, rows, wire))
        }
        _ => return Err(Error::Corrupt),
    };
    let hash = descriptor_hash(tx, &bytes[..at])?;
    if value::take(bytes, &mut at, 32).map_err(admission::value_error)? != hash {
        return Err(Error::Corrupt);
    }
    value::finish(bytes, at).map_err(admission::value_error)?;
    Ok(result)
}
