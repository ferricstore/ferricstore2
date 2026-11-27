//! A group's snapshots (ADR-018 phase 3): what the machine captured at an
//! applied position, with that position and the membership then in force.
//!
//! A snapshot's bytes are its metadata's length (`u32`), the metadata (the
//! last applied log id, the membership's log id, each a presence byte and a
//! term, node and index, then the membership) and the machine's bytes. The
//! store keeps the newest by the last applied index.

use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use openraft::storage::RaftSnapshotBuilder;
use openraft::type_config::alias::{LogIdOf, SnapshotMetaOf, SnapshotOf};
use openraft::{Snapshot, SnapshotMeta, StoredMembership};
use tokio::sync::oneshot;
use wf_ports::consensus::log::{ConsensusLogError, EntryId, SnapshotStore};

use super::{Config, codec};

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "malformed group snapshot")
}

fn put_log_id(out: &mut Vec<u8>, log_id: Option<&LogIdOf<Config>>) {
    match log_id.map(codec::entry_id) {
        Some(id) => {
            out.push(1);
            for part in [id.term, id.node, id.index] {
                out.extend_from_slice(&part.to_be_bytes());
            }
        }
        None => out.push(0),
    }
}

fn take_log_id(bytes: &mut &[u8]) -> io::Result<Option<LogIdOf<Config>>> {
    let (&present, rest) = bytes.split_first().ok_or_else(invalid)?;
    *bytes = rest;
    if present == 0 {
        return Ok(None);
    }
    let mut part = || -> io::Result<u64> {
        let (value, rest) = bytes.split_first_chunk::<8>().ok_or_else(invalid)?;
        *bytes = rest;
        Ok(u64::from_be_bytes(*value))
    };
    let id = EntryId {
        term: part()?,
        node: part()?,
        index: part()?,
    };
    Ok(Some(codec::log_id(id)))
}

/// The metadata's bytes.
pub(super) fn meta_bytes(meta: &SnapshotMetaOf<Config>) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    put_log_id(&mut out, meta.last_log_id.as_ref());
    put_log_id(&mut out, meta.last_membership.log_id().as_ref());
    codec::put_membership(&mut out, meta.last_membership.membership())?;
    Ok(out)
}

/// Metadata from its bytes.
pub(super) fn parse_meta(mut bytes: &[u8]) -> io::Result<SnapshotMetaOf<Config>> {
    let last_log_id = take_log_id(&mut bytes)?;
    let membership_log_id = take_log_id(&mut bytes)?;
    let membership = codec::membership(bytes).ok_or_else(invalid)?;
    Ok(SnapshotMeta {
        last_log_id,
        last_membership: StoredMembership::new(membership_log_id, membership),
    })
}

/// Saves a snapshot as the newest.
pub(super) fn save(
    store: &dyn SnapshotStore,
    meta: &SnapshotMetaOf<Config>,
    data: &[u8],
) -> io::Result<()> {
    let meta_bytes = meta_bytes(meta)?;
    let head = u32::try_from(meta_bytes.len())
        .map_err(|_| invalid())?
        .to_be_bytes();
    let id = meta.last_log_id.map_or(0, |log_id| log_id.index);
    store
        .save(id, &[&head, &meta_bytes, data])
        .map_err(|error| io::Error::other(error.to_string()))
}

/// Saves a snapshot as the newest on a blocking thread: a save writes and
/// syncs the whole snapshot, which must not hold up the runtime that sends
/// the member's heartbeats and votes.
pub(super) async fn save_aside(
    store: Arc<dyn SnapshotStore>,
    meta: SnapshotMetaOf<Config>,
    data: Arc<Vec<u8>>,
) -> io::Result<()> {
    tokio::task::spawn_blocking(move || save(store.as_ref(), &meta, &data))
        .await
        .map_err(io::Error::other)?
}

/// The newest stored snapshot's metadata and machine bytes.
pub(super) fn newest(
    store: &dyn SnapshotStore,
) -> io::Result<Option<(SnapshotMetaOf<Config>, Vec<u8>)>> {
    let Some((_, body)) = store
        .newest()
        .map_err(|error| io::Error::other(error.to_string()))?
    else {
        return Ok(None);
    };
    let (length, rest) = body.split_first_chunk::<4>().ok_or_else(invalid)?;
    let length = usize::try_from(u32::from_be_bytes(*length)).map_err(|_| invalid())?;
    let (meta, data) = rest.split_at_checked(length).ok_or_else(invalid)?;
    let meta = parse_meta(meta)?;
    Ok(Some((meta, data.to_vec())))
}

/// The newest snapshot, shared by the bridge and its builders.
pub(super) type Current = Arc<Mutex<Option<(SnapshotMetaOf<Config>, Arc<Vec<u8>>)>>>;

/// A snapshot being captured: the machine was asked for its bytes at the
/// applied position `meta` names.
pub(crate) struct Capture {
    pub(super) meta: SnapshotMetaOf<Config>,
    pub(super) bytes: Option<oneshot::Receiver<Result<Vec<u8>, ConsensusLogError>>>,
    pub(super) store: Arc<dyn SnapshotStore>,
    pub(super) current: Current,
}

impl RaftSnapshotBuilder<Config> for Capture {
    type SnapshotData = Vec<u8>;

    async fn build_snapshot(&mut self) -> Result<SnapshotOf<Config, Vec<u8>>, io::Error> {
        let receiver = self.bytes.take().ok_or_else(invalid)?;
        let data = receiver
            .await
            .map_err(io::Error::other)?
            .map_err(|error| io::Error::other(error.to_string()))?;
        let data = Arc::new(data);
        save_aside(
            Arc::clone(&self.store),
            self.meta.clone(),
            Arc::clone(&data),
        )
        .await?;
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) =
            Some((self.meta.clone(), Arc::clone(&data)));
        Ok(Snapshot {
            meta: self.meta.clone(),
            snapshot: data.as_ref().clone(),
        })
    }
}
