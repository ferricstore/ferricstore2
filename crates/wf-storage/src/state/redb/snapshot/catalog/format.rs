//! Fixed catalog bytes, with exact scope-bound checksum and no payload owner.

use super::super::super::{admission, store::RedbStateTransaction};
use super::{SnapshotCatalog, SnapshotTransition};
use crate::state::value;
use sha2::{Digest, Sha256};
use wf_ports::{
    log::{RaftLogState, RaftVote},
    state::StateTransactionError as Error,
};
use wf_types::version::{AppliedPosition, LogIdentity};

fn hash(tx: &RedbStateTransaction<'_>, bytes: &[u8]) -> Result<[u8; 32], Error> {
    let mut hash = Sha256::new();
    hash.update(b"wf-state-snapshot-catalog-v1");
    for part in [tx.group.as_bytes(), tx.recovery.as_bytes()] {
        hash.update(
            u16::try_from(part.len())
                .map_err(|_| Error::Limit)?
                .to_be_bytes(),
        );
        hash.update(part);
    }
    hash.update(bytes);
    Ok(hash.finalize().into())
}
fn take<'a>(bytes: &'a [u8], at: &mut usize, n: usize) -> Result<&'a [u8], Error> {
    value::take(bytes, at, n).map_err(admission::value_error)
}
fn byte(bytes: &[u8], at: &mut usize) -> Result<u8, Error> {
    Ok(take(bytes, at, 1)?[0])
}
fn number(bytes: &[u8], at: &mut usize) -> Result<u64, Error> {
    Ok(u64::from_be_bytes(
        take(bytes, at, 8)?.try_into().map_err(|_| Error::Corrupt)?,
    ))
}
fn identity(bytes: &[u8], at: &mut usize) -> Result<LogIdentity, Error> {
    Ok(LogIdentity::new(
        number(bytes, at)?,
        number(bytes, at)?,
        AppliedPosition::new(number(bytes, at)?),
    ))
}
fn selector(bytes: &[u8], at: &mut usize) -> Result<Option<u8>, Error> {
    match byte(bytes, at)? {
        0 => Ok(None),
        1 => Ok(Some(0)),
        2 => Ok(Some(1)),
        _ => Err(Error::Corrupt),
    }
}
fn pending(bytes: &[u8], at: &mut usize) -> Result<Option<SnapshotTransition>, Error> {
    match byte(bytes, at)? {
        0 => return Ok(None),
        1 => {}
        _ => return Err(Error::Corrupt),
    }
    let received = byte(bytes, at)?;
    let flags = byte(bytes, at)?;
    if received > 1 || flags & !15 != 0 {
        return Err(Error::Corrupt);
    }
    let vote = if flags & 1 != 0 {
        let term = number(bytes, at)?;
        let node = number(bytes, at)?;
        let committed = byte(bytes, at)?;
        if committed > 1 {
            return Err(Error::Corrupt);
        }
        Some(RaftVote::new(term, node, committed == 1))
    } else {
        None
    };
    let committed = if flags & 2 != 0 {
        Some(identity(bytes, at)?)
    } else {
        None
    };
    let tail = if flags & 4 != 0 {
        Some(identity(bytes, at)?)
    } else {
        None
    };
    let anchor = if flags & 8 != 0 {
        Some((
            identity(bytes, at)?,
            take(bytes, at, 32)?
                .try_into()
                .map_err(|_| Error::Corrupt)?,
        ))
    } else {
        None
    };
    let before = RaftLogState::new_with_anchor(vote, committed, tail, anchor);
    let position = identity(bytes, at)?;
    let id = take(bytes, at, 32)?
        .try_into()
        .map_err(|_| Error::Corrupt)?;
    let result = if received == 1 {
        SnapshotTransition::received(before, position, id)
    } else {
        SnapshotTransition::local(before, position, id)
    };
    result.map(Some).map_err(|_| Error::Corrupt)
}
pub(in crate::state::redb::snapshot) fn decode(
    tx: &RedbStateTransaction<'_>,
    bytes: &[u8],
) -> Result<SnapshotCatalog, Error> {
    if !(43..=318).contains(&bytes.len()) {
        return Err(Error::Corrupt);
    }
    let mut at = 0;
    if take(bytes, &mut at, 6)? != b"FWSD\0\x03" {
        return Err(Error::Corrupt);
    }
    let current = selector(bytes, &mut at)?;
    let anchor = selector(bytes, &mut at)?;
    let mut slots = [None, None];
    for slot in &mut slots {
        match byte(bytes, &mut at)? {
            0 => {}
            1 => {
                let rows = usize::try_from(number(bytes, &mut at)?).map_err(|_| Error::Limit)?;
                let wire = usize::try_from(number(bytes, &mut at)?).map_err(|_| Error::Limit)?;
                let id = take(bytes, &mut at, 32)?
                    .try_into()
                    .map_err(|_| Error::Corrupt)?;
                if rows < 3 || wire == 0 {
                    return Err(Error::Corrupt);
                }
                *slot = Some((id, rows, wire));
            }
            _ => return Err(Error::Corrupt),
        }
    }
    let pending = pending(bytes, &mut at)?;
    let expected = hash(tx, &bytes[..at])?;
    if take(bytes, &mut at, 32)? != expected || at != bytes.len() {
        return Err(Error::Corrupt);
    }
    let result = SnapshotCatalog {
        slots,
        current,
        anchor,
        pending,
    };
    result.check()?;
    Ok(result)
}
fn push(out: &mut [u8], at: &mut usize, bytes: &[u8]) {
    out[*at..*at + bytes.len()].copy_from_slice(bytes);
    *at += bytes.len();
}
fn put_id(out: &mut [u8], at: &mut usize, id: LogIdentity) {
    for n in [id.term(), id.leader_node(), id.index().get()] {
        push(out, at, &n.to_be_bytes());
    }
}
pub(super) fn encode(
    tx: &RedbStateTransaction<'_>,
    catalog: &SnapshotCatalog,
    out: &mut [u8; 318],
) -> Result<usize, Error> {
    catalog.check()?;
    let mut at = 0;
    push(out, &mut at, b"FWSD\0\x03");
    push(
        out,
        &mut at,
        &[
            catalog.current.map_or(0, |s| s + 1),
            catalog.anchor.map_or(0, |s| s + 1),
        ],
    );
    for slot in catalog.slots {
        push(out, &mut at, &[u8::from(slot.is_some())]);
        if let Some((id, rows, wire)) = slot {
            for n in [rows, wire] {
                push(
                    out,
                    &mut at,
                    &u64::try_from(n).map_err(|_| Error::Limit)?.to_be_bytes(),
                );
            }
            push(out, &mut at, &id);
        }
    }
    push(out, &mut at, &[u8::from(catalog.pending.is_some())]);
    if let Some(t) = catalog.pending {
        let old = t.before();
        push(
            out,
            &mut at,
            &[
                u8::from(t.is_received()),
                u8::from(old.vote().is_some())
                    | u8::from(old.committed().is_some()) << 1
                    | u8::from(old.last_log().is_some()) << 2
                    | u8::from(old.last_purged().is_some()) << 3,
            ],
        );
        if let Some(v) = old.vote() {
            push(out, &mut at, &v.term().to_be_bytes());
            push(out, &mut at, &v.node().to_be_bytes());
            push(out, &mut at, &[u8::from(v.committed())]);
        }
        for id in [old.committed(), old.last_log()].into_iter().flatten() {
            put_id(out, &mut at, id);
        }
        if let Some((p, id)) = old.last_purged().zip(old.purged_snapshot_id()) {
            put_id(out, &mut at, p);
            push(out, &mut at, &id);
        }
        put_id(out, &mut at, t.position());
        push(out, &mut at, t.id());
    }
    let checksum = hash(tx, &out[..at])?;
    push(out, &mut at, &checksum);
    Ok(at)
}
