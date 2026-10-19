//! Closed scoped envelopes and checked logical accounting, without body owners.

use redb::TableDefinition;
use sha2::{Digest, Sha256};
use wf_ports::log::{RaftLogError, RaftLogState, RaftVote};
use wf_types::identity::{GroupId, RecoveryId};
use wf_types::version::{AppliedPosition, LogIdentity};

pub(super) const META: TableDefinition<'static, &[u8], &[u8]> =
    TableDefinition::new("wf_raft_meta_v1");
pub(super) const LOGS: TableDefinition<'static, u64, &[u8]> =
    TableDefinition::new("wf_raft_log_v1");
pub(super) const MANIFEST: &[u8] = b"manifest";
pub(super) const STATE: &[u8] = b"state";

pub(super) fn sum(values: &[usize]) -> Result<usize, RaftLogError> {
    values.iter().try_fold(0_usize, |total, value| {
        total.checked_add(*value).ok_or(RaftLogError::Limit)
    })
}

pub(super) fn allocate(size: usize) -> Result<Vec<u8>, RaftLogError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(size)
        .map_err(|_| RaftLogError::Allocation)?;
    Ok(output)
}

fn namespace(
    hash: &mut Sha256,
    group: &GroupId,
    recovery: &RecoveryId,
) -> Result<(), RaftLogError> {
    hash.update(b"wf-raft-log-v1");
    for bytes in [group.as_bytes(), recovery.as_bytes()] {
        let length = u16::try_from(bytes.len()).map_err(|_| RaftLogError::Limit)?;
        hash.update(length.to_be_bytes());
        hash.update(bytes);
    }
    Ok(())
}

fn digest(prefix: &[u8], scope: Option<(&GroupId, &RecoveryId)>) -> Result<[u8; 32], RaftLogError> {
    let mut hash = Sha256::new();
    if let Some((group, recovery)) = scope {
        namespace(&mut hash, group, recovery)?;
    }
    hash.update(prefix);
    Ok(hash.finalize().into())
}

fn prefix(wire: &[u8], magic: [u8; 4]) -> Result<&[u8], RaftLogError> {
    if wire.len() < 38 || wire[..4] != magic || wire[4..6] != [0, 1] {
        return Err(RaftLogError::Corrupt);
    }
    Ok(&wire[..wire.len() - 32])
}

fn verify(wire: &[u8], scope: Option<(&GroupId, &RecoveryId)>) -> Result<(), RaftLogError> {
    let end = wire.len().checked_sub(32).ok_or(RaftLogError::Corrupt)?;
    if wire[end..] != digest(&wire[..end], scope)? {
        return Err(RaftLogError::Corrupt);
    }
    Ok(())
}

fn take<const N: usize>(bytes: &[u8], offset: &mut usize) -> Result<[u8; N], RaftLogError> {
    let end = offset.checked_add(N).ok_or(RaftLogError::Corrupt)?;
    let value = bytes.get(*offset..end).ok_or(RaftLogError::Corrupt)?;
    *offset = end;
    value.try_into().map_err(|_| RaftLogError::Corrupt)
}

fn raw<'a>(bytes: &'a [u8], offset: &mut usize) -> Result<&'a [u8], RaftLogError> {
    let length = usize::from(u16::from_be_bytes(take(bytes, offset)?));
    if length == 0 || length > 256 {
        return Err(RaftLogError::Corrupt);
    }
    let end = offset.checked_add(length).ok_or(RaftLogError::Corrupt)?;
    let value = bytes.get(*offset..end).ok_or(RaftLogError::Corrupt)?;
    *offset = end;
    Ok(value)
}

pub(super) fn manifest(group: &GroupId, recovery: &RecoveryId) -> Result<Vec<u8>, RaftLogError> {
    let mut wire = allocate(sum(&[
        42,
        group.as_bytes().len(),
        recovery.as_bytes().len(),
    ])?)?;
    wire.extend_from_slice(b"FWRM\0\x01");
    for bytes in [group.as_bytes(), recovery.as_bytes()] {
        let length = u16::try_from(bytes.len()).map_err(|_| RaftLogError::Limit)?;
        wire.extend_from_slice(&length.to_be_bytes());
        wire.extend_from_slice(bytes);
    }
    let hash = digest(&wire, None)?;
    wire.extend_from_slice(&hash);
    Ok(wire)
}

pub(super) fn check_manifest(
    wire: &[u8],
    group: &GroupId,
    recovery: &RecoveryId,
) -> Result<(), RaftLogError> {
    let body = prefix(wire, *b"FWRM")?;
    let mut offset = 6;
    let actual_group = raw(body, &mut offset)?;
    let actual_recovery = raw(body, &mut offset)?;
    if offset != body.len() {
        return Err(RaftLogError::Corrupt);
    }
    verify(wire, None)?;
    if actual_group != group.as_bytes() || actual_recovery != recovery.as_bytes() {
        return Err(RaftLogError::Conflict);
    }
    Ok(())
}

fn put_id(output: &mut Vec<u8>, id: LogIdentity) {
    output.extend_from_slice(&id.term().to_be_bytes());
    output.extend_from_slice(&id.leader_node().to_be_bytes());
    output.extend_from_slice(&id.index().get().to_be_bytes());
}

fn read_id(bytes: &[u8], offset: &mut usize) -> Result<LogIdentity, RaftLogError> {
    Ok(LogIdentity::new(
        u64::from_be_bytes(take(bytes, offset)?),
        u64::from_be_bytes(take(bytes, offset)?),
        AppliedPosition::new(u64::from_be_bytes(take(bytes, offset)?)),
    ))
}

pub(super) fn state(
    state: RaftLogState,
    group: &GroupId,
    recovery: &RecoveryId,
) -> Result<Vec<u8>, RaftLogError> {
    let size = 39
        + usize::from(state.vote().is_some()) * 17
        + usize::from(state.committed().is_some()) * 24
        + usize::from(state.last_log().is_some()) * 24;
    let mut wire = allocate(size)?;
    wire.extend_from_slice(b"FWLS\0\x01");
    wire.push(
        u8::from(state.vote().is_some())
            | (u8::from(state.committed().is_some()) << 1)
            | (u8::from(state.last_log().is_some()) << 2),
    );
    if let Some(vote) = state.vote() {
        wire.extend_from_slice(&vote.term().to_be_bytes());
        wire.extend_from_slice(&vote.node().to_be_bytes());
        wire.push(u8::from(vote.committed()));
    }
    for id in [state.committed(), state.last_log()].into_iter().flatten() {
        put_id(&mut wire, id);
    }
    let hash = digest(&wire, Some((group, recovery)))?;
    wire.extend_from_slice(&hash);
    Ok(wire)
}

pub(super) fn decode_state(
    wire: &[u8],
    group: &GroupId,
    recovery: &RecoveryId,
) -> Result<RaftLogState, RaftLogError> {
    let body = prefix(wire, *b"FWLS")?;
    let mut offset = 6;
    let [flags] = take(body, &mut offset)?;
    if flags & !7 != 0 {
        return Err(RaftLogError::Corrupt);
    }
    let vote = if flags & 1 != 0 {
        let term = u64::from_be_bytes(take(body, &mut offset)?);
        let node = u64::from_be_bytes(take(body, &mut offset)?);
        let [committed] = take(body, &mut offset)?;
        if committed > 1 {
            return Err(RaftLogError::Corrupt);
        }
        Some(RaftVote::new(term, node, committed != 0))
    } else {
        None
    };
    let committed = if flags & 2 != 0 {
        Some(read_id(body, &mut offset)?)
    } else {
        None
    };
    let last = if flags & 4 != 0 {
        Some(read_id(body, &mut offset)?)
    } else {
        None
    };
    if offset != body.len()
        || committed.is_some_and(|id| last.is_none_or(|tail| id.index() > tail.index()))
    {
        return Err(RaftLogError::Corrupt);
    }
    verify(wire, Some((group, recovery)))?;
    Ok(RaftLogState::new(vote, committed, last))
}

pub(super) fn entry(
    id: LogIdentity,
    body: &[u8],
    group: &GroupId,
    recovery: &RecoveryId,
) -> Result<Vec<u8>, RaftLogError> {
    let mut wire = allocate(sum(&[70, body.len()])?)?;
    wire.extend_from_slice(b"FWRL\0\x01");
    put_id(&mut wire, id);
    wire.extend_from_slice(
        &u64::try_from(body.len())
            .map_err(|_| RaftLogError::Limit)?
            .to_be_bytes(),
    );
    wire.extend_from_slice(body);
    let hash = digest(&wire, Some((group, recovery)))?;
    wire.extend_from_slice(&hash);
    Ok(wire)
}

pub(super) fn decode_entry<'a>(
    wire: &'a [u8],
    index: u64,
    group: &GroupId,
    recovery: &RecoveryId,
    maximum_body_bytes: usize,
) -> Result<(LogIdentity, &'a [u8]), RaftLogError> {
    let payload = prefix(wire, *b"FWRL")?;
    let mut offset = 6;
    let id = read_id(payload, &mut offset)?;
    let length = usize::try_from(u64::from_be_bytes(take(payload, &mut offset)?))
        .map_err(|_| RaftLogError::Corrupt)?;
    let body = payload.get(offset..).ok_or(RaftLogError::Corrupt)?;
    if id.index().get() != index || body.len() != length {
        return Err(RaftLogError::Corrupt);
    }
    if length > maximum_body_bytes {
        return Err(RaftLogError::Limit);
    }
    verify(wire, Some((group, recovery)))?;
    Ok((id, body))
}

pub(super) fn follows(previous: LogIdentity, next: LogIdentity) -> bool {
    previous.index().checked_next() == Some(next.index())
        && (previous.term(), previous.leader_node()) <= (next.term(), next.leader_node())
}

pub(super) fn storage_error(error: &redb::StorageError) -> RaftLogError {
    match error {
        redb::StorageError::Corrupted(_) => RaftLogError::Corrupt,
        redb::StorageError::ValueTooLarge(_) => RaftLogError::Limit,
        _ => RaftLogError::Unavailable,
    }
}

pub(super) fn database_error(error: redb::DatabaseError) -> RaftLogError {
    match error {
        redb::DatabaseError::Storage(error) => storage_error(&error),
        _ => RaftLogError::Unavailable,
    }
}

pub(super) fn transaction_error(error: redb::TransactionError) -> RaftLogError {
    match error {
        redb::TransactionError::Storage(error) => storage_error(&error),
        _ => RaftLogError::Unavailable,
    }
}

pub(super) fn table_error(error: redb::TableError) -> RaftLogError {
    match error {
        redb::TableError::Storage(error) => storage_error(&error),
        _ => RaftLogError::Corrupt,
    }
}
