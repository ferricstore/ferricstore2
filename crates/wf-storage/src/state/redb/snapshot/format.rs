//! Canonical framed emission and scope-bound descriptor bytes.

use super::super::store::RedbStateTransaction;
use super::{KEY, SnapshotError, SnapshotLimits, admission, io};
use crate::state::value;
use redb::{ReadableTable, TableHandle};
use sha2::{Digest, Sha256};
use wf_ports::state::StateTransactionError as Error;

fn sizes(tx: &RedbStateTransaction<'_>, limits: SnapshotLimits) -> Result<(usize, usize), Error> {
    let mut rows = 0_usize;
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
            pair(key.value().len(), value.value().len(), limits)?;
            rows = admission::total(rows, [1])?;
            wire = admission::total(wire, [12, key.value().len(), value.value().len()])?;
            if rows > limits.maximum_rows() || wire > limits.maximum_encoded_bytes() {
                return Err(Error::Limit);
            }
        }
    }
    Ok((rows, wire))
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
    counts: (usize, usize),
) -> Result<(), SnapshotError> {
    push(tx, output, limits, hash, b"FWSP\0\x01")?;
    for part in [tx.group.as_bytes(), tx.recovery.as_bytes()] {
        let n = u16::try_from(part.len()).map_err(|_| SnapshotError::Limit)?;
        push(tx, output, limits, hash, &n.to_be_bytes())?;
        push(tx, output, limits, hash, part)?;
    }
    let rows = counts.0.checked_sub(2).ok_or(SnapshotError::Malformed)?;
    for n in [2, rows, counts.1] {
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
            push(tx, output, limits, hash, value.value())?;
        }
    }
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
    out: &mut [u8; 87],
) -> Result<usize, Error> {
    out[..6].copy_from_slice(b"FWSD\0\x01");
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
    if value::take(bytes, &mut at, 6).map_err(admission::value_error)? != b"FWSD\0\x01" {
        return Err(Error::Corrupt);
    }
    let result = match value::read_u8(bytes, &mut at).map_err(admission::value_error)? {
        0 => None,
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
