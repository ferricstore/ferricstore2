//! Actual streaming ingestion into one uncommitted, initially empty writer.

use super::super::store::{RedbStateStore, RedbStateTransaction};
use super::{SnapshotError, SnapshotLimits, admission, format, io, validate};
use redb::ReadableTable;
use sha2::{Digest, Sha256};
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_ports::state::StateTransactionError as Error;

fn piece(
    tx: &RedbStateTransaction<'_>,
    input: &mut impl std::io::Read,
    limits: SnapshotLimits,
    state: &mut (Sha256, usize, u16),
    bytes: &mut [u8],
) -> Result<(), SnapshotError> {
    io::read(tx, input, bytes, limits)?;
    state.1 = state
        .1
        .checked_add(bytes.len())
        .ok_or(SnapshotError::Limit)?;
    state.0.update(bytes);
    Ok(())
}

fn header(
    tx: &RedbStateTransaction<'_>,
    input: &mut impl std::io::Read,
    limits: SnapshotLimits,
    state: &mut (Sha256, usize, u16),
) -> Result<(usize, usize, u16), SnapshotError> {
    let mut magic = [0; 6];
    piece(tx, input, limits, state, &mut magic)?;
    if &magic[..4] != b"FWSP" {
        return Err(SnapshotError::Malformed);
    }
    let version = u16::from_be_bytes([magic[4], magic[5]]);
    if !matches!(version, 1..=4) {
        return Err(SnapshotError::UnsupportedVersion);
    }
    for expected in [tx.group.as_bytes(), tx.recovery.as_bytes()] {
        let mut size = [0; 2];
        piece(tx, input, limits, state, &mut size)?;
        let n = usize::from(u16::from_be_bytes(size));
        if n == 0 || n > 256 {
            return Err(SnapshotError::Malformed);
        }
        let mut bytes = [0; 256];
        piece(tx, input, limits, state, &mut bytes[..n])?;
        if &bytes[..n] != expected {
            return Err(SnapshotError::NamespaceMismatch);
        }
    }
    let mut counts = [0; 24];
    piece(tx, input, limits, state, &mut counts)?;
    let mut values = [0_usize; 3];
    for (target, bytes) in values.iter_mut().zip(counts.as_chunks::<8>().0) {
        *target = usize::try_from(u64::from_be_bytes(*bytes)).map_err(|_| SnapshotError::Limit)?;
    }
    let metadata = if version >= 2 { 3 } else { 2 };
    if values[0] != metadata {
        return Err(SnapshotError::Malformed);
    }
    let rows = values[1]
        .checked_add(metadata)
        .ok_or(SnapshotError::Limit)?;
    if rows > limits.maximum_rows() || values[2] > limits.maximum_encoded_bytes() {
        return Err(SnapshotError::Limit);
    }
    let minimum = rows
        .checked_mul(12)
        .and_then(|n| n.checked_add(state.1))
        .and_then(|n| n.checked_add(32))
        .ok_or(SnapshotError::Limit)?;
    if values[1] == 0 || values[2] < minimum {
        return Err(SnapshotError::Malformed);
    }
    Ok((values[1], values[2], version))
}

fn row(
    tx: &RedbStateTransaction<'_>,
    input: &mut impl std::io::Read,
    limits: SnapshotLimits,
    state: &mut (Sha256, usize, u16),
    buffer: &mut (Vec<u8>, usize),
    wire: usize,
    position: (bool, usize, u16),
) -> Result<(), SnapshotError> {
    if wire.checked_sub(state.1).is_none_or(|n| n < 44) {
        return Err(SnapshotError::Malformed);
    }
    let mut lengths = [0_u8; 12];
    piece(tx, input, limits, state, &mut lengths)?;
    let key_size = usize::try_from(u32::from_be_bytes(
        lengths[..4]
            .try_into()
            .map_err(|_| SnapshotError::Malformed)?,
    ))
    .map_err(|_| SnapshotError::Limit)?;
    let value_size = usize::try_from(u64::from_be_bytes(
        lengths[4..]
            .try_into()
            .map_err(|_| SnapshotError::Malformed)?,
    ))
    .map_err(|_| SnapshotError::Limit)?;
    format::pair(key_size, value_size, limits).map_err(|_| SnapshotError::Limit)?;
    let size = key_size
        .checked_add(value_size)
        .ok_or(SnapshotError::Limit)?;
    if size
        > wire
            .checked_sub(state.1)
            .and_then(|n| n.checked_sub(32))
            .ok_or(SnapshotError::Malformed)?
    {
        return Err(SnapshotError::Malformed);
    }
    if size > buffer.1 {
        tx.temporary(size.checked_add(512).ok_or(SnapshotError::Limit)?)
            .map_err(|_| SnapshotError::Limit)?;
        buffer.0.clear();
        buffer
            .0
            .try_reserve_exact(size)
            .map_err(|_| SnapshotError::Allocation)?;
        buffer.1 = size;
    }
    buffer.0.resize(size, 0);
    piece(tx, input, limits, state, &mut buffer.0)?;
    let (key, value) = buffer.0.split_at(key_size);
    let expected: &[u8] = if position.1 == 0 {
        b"manifest"
    } else if position.1 == 1 {
        b"membership"
    } else {
        b"origin"
    };
    if position.0 && position.1 == 0 {
        let layout = match value.get(4..6) {
            Some([high, low]) => u16::from_be_bytes([*high, *low]),
            _ => return Err(SnapshotError::Malformed),
        };
        if !matches!((position.2, layout), (4, 7 | 8) | (3, 6) | (2, 5) | (1, 3)) {
            return Err(SnapshotError::Malformed);
        }
        state.2 = layout;
    }
    if position.0 && key != expected {
        return Err(SnapshotError::Malformed);
    }
    insert(tx, key, value, position.0)
}

fn insert(
    tx: &RedbStateTransaction<'_>,
    key: &[u8],
    value: &[u8],
    meta: bool,
) -> Result<(), SnapshotError> {
    let definitions = if meta {
        (super::super::META, super::META)
    } else {
        (super::super::ROWS, super::ROWS)
    };
    tx.step().map_err(SnapshotError::Stage)?;
    let mut live = tx
        .write
        .open_table(definitions.0)
        .map_err(admission::table_error)
        .map_err(SnapshotError::Stage)?;
    tx.step().map_err(SnapshotError::Stage)?;
    if let Some((previous, bytes)) = live
        .last()
        .map_err(|e| SnapshotError::Stage(admission::storage_error(&e)))?
    {
        tx.recovery_row(previous.value(), bytes.value())
            .map_err(SnapshotError::Stage)?;
        if previous.value() >= key {
            return Err(SnapshotError::Malformed);
        }
    }
    tx.step().map_err(SnapshotError::Stage)?;
    tx.recovery_row(key, value).map_err(SnapshotError::Stage)?;
    live.insert(key, value)
        .map_err(|e| SnapshotError::Stage(admission::storage_error(&e)))?;
    tx.step().map_err(SnapshotError::Stage)?;
    let mut frozen = tx
        .write
        .open_table(definitions.1)
        .map_err(admission::table_error)
        .map_err(SnapshotError::Stage)?;
    tx.step().map_err(SnapshotError::Stage)?;
    tx.recovery_row(key, value).map_err(SnapshotError::Stage)?;
    frozen
        .insert(key, value)
        .map_err(|e| SnapshotError::Stage(admission::storage_error(&e)))?;
    Ok(())
}

fn fresh(tx: &RedbStateTransaction<'_>) -> Result<(), Error> {
    tx.step()?;
    let mut tables = tx
        .write
        .list_tables()
        .map_err(|e| admission::storage_error(&e))?;
    tx.step()?;
    if tables.next().is_some() {
        return Err(Error::NamespaceConflict);
    }
    tx.step()?;
    let mut multimaps = tx
        .write
        .list_multimap_tables()
        .map_err(|e| admission::storage_error(&e))?;
    tx.step()?;
    if multimaps.next().is_some() {
        return Err(Error::NamespaceConflict);
    }
    Ok(())
}

pub(super) fn receive(
    store: &RedbStateStore,
    input: &mut impl std::io::Read,
    limits: SnapshotLimits,
) -> Result<([u8; 32], usize, usize, u16), SnapshotError> {
    let mut tx = super::transaction(store, limits, store.limits).map_err(SnapshotError::Stage)?;
    fresh(&tx).map_err(SnapshotError::Stage)?;
    let mut state = (Sha256::new(), 0, 0);
    let (rows, wire, version) = header(&tx, input, limits, &mut state)?;
    for definition in [
        super::super::META,
        super::super::ROWS,
        super::META,
        super::ROWS,
    ] {
        tx.step().map_err(SnapshotError::Stage)?;
        drop(
            tx.write
                .open_table(definition)
                .map_err(admission::table_error)
                .map_err(SnapshotError::Stage)?,
        );
    }
    {
        let mut buffer = (Vec::new(), 0);
        for (meta, count) in [(true, if version >= 2 { 3 } else { 2 }), (false, rows)] {
            for index in 0..count {
                row(
                    &tx,
                    input,
                    limits,
                    &mut state,
                    &mut buffer,
                    wire,
                    (meta, index, version),
                )?;
            }
        }
    }
    if state.1.checked_add(32) != Some(wire) {
        return Err(SnapshotError::Malformed);
    }
    let mut footer = [0; 32];
    io::read(&tx, input, &mut footer, limits)?;
    if state.0.clone().finalize().as_slice() != footer {
        return Err(SnapshotError::Malformed);
    }
    state.0.update(footer);
    io::eof(&tx, input, limits)?;
    let facts = (
        state.0.finalize().into(),
        rows + if version >= 2 { 3 } else { 2 },
        wire,
    );
    if version >= 2 {
        super::policy::upgrade_catalog(
            &tx,
            &super::catalog::SnapshotCatalog {
                slots: [Some(facts), None],
                current: Some(0),
                anchor: None,
                pending: None,
            },
        )
        .map_err(SnapshotError::Stage)?;
    } else {
        super::put_descriptor(&tx, Some(facts)).map_err(SnapshotError::Stage)?;
    }
    let mut charge = PolicySchemaCharge::new(store.limits.schema());
    if validate::complete(&mut tx, &mut charge, limits).map_err(SnapshotError::Stage)?
        != Some(facts)
    {
        return Err(SnapshotError::Stage(Error::Corrupt));
    }
    super::commit(tx).map_err(SnapshotError::Stage)?;
    Ok((facts.0, facts.1, facts.2, state.2))
}
