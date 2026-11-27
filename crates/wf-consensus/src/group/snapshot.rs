//! A group's snapshots (ADR-018 phase 3): what the machine captured at an
//! applied position, with that position and the membership then in force.
//!
//! A snapshot is a file in the member's snapshot store, named by its last
//! applied index ([`Stored`]), and is never in memory whole: the machine's
//! capture streams into it, a transfer streams it out, an install streams
//! it into the machine. Its bytes are its metadata's length (`u32`), the
//! metadata (the last applied log id, the membership's log id, each a
//! presence byte and a term, node and index, then the membership) and the
//! machine's bytes. The store keeps the newest adopted one.

use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex, PoisonError};

use openraft::storage::RaftSnapshotBuilder;
use openraft::type_config::alias::{LogIdOf, SnapshotMetaOf, SnapshotOf};
use openraft::{Snapshot, SnapshotMeta, StoredMembership};
use tokio::sync::oneshot;
use wf_ports::consensus::log::{
    Capture as MachineCapture, ConsensusLogError, EntryId, SnapshotSource, SnapshotStore,
};

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

/// A snapshot a member holds: snapshot `id` (its last applied index) in
/// its store, `length` bytes long. Clones name the same file.
#[derive(Clone)]
pub struct Stored {
    store: Arc<dyn SnapshotStore>,
    id: u64,
    length: u64,
}

impl Stored {
    /// Snapshot `id` of `length` bytes in `store`.
    #[must_use]
    pub fn new(store: Arc<dyn SnapshotStore>, id: u64, length: u64) -> Self {
        Self { store, id, length }
    }

    /// Its id: its last applied index.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Its length in bytes.
    #[must_use]
    pub const fn length(&self) -> u64 {
        self.length
    }

    /// The store that holds it.
    #[must_use]
    pub fn store(&self) -> &Arc<dyn SnapshotStore> {
        &self.store
    }

    /// Reads it from its start, checked as it is read.
    ///
    /// # Errors
    /// The store's refusal or failure.
    pub fn open(&self) -> io::Result<Box<dyn SnapshotSource>> {
        self.store.open(self.id).map_err(io::Error::other)
    }
}

impl std::fmt::Debug for Stored {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "snapshot {} ({} bytes)", self.id, self.length)
    }
}

/// The id a snapshot with `meta` is stored under.
pub(super) fn id_of(meta: &SnapshotMetaOf<Config>) -> u64 {
    meta.last_log_id.map_or(0, |log_id| log_id.index)
}

/// Writes a snapshot's head: its metadata's length and the metadata.
pub(super) fn write_head(out: &mut dyn Write, meta: &SnapshotMetaOf<Config>) -> io::Result<()> {
    let bytes = meta_bytes(meta)?;
    let length = u32::try_from(bytes.len()).map_err(|_| invalid())?;
    out.write_all(&length.to_be_bytes())?;
    out.write_all(&bytes)
}

/// Reads a snapshot's head, leaving `input` at the machine's bytes.
pub(super) fn read_head(input: &mut dyn Read) -> io::Result<SnapshotMetaOf<Config>> {
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let length = usize::try_from(u32::from_be_bytes(length)).map_err(|_| invalid())?;
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes)?;
    parse_meta(&bytes)
}

fn other(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

/// Runs blocking snapshot work (file reads and writes) off the runtime
/// that sends the member's heartbeats and votes.
pub(super) async fn aside<T: Send + 'static>(
    work: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> io::Result<T> {
    tokio::task::spawn_blocking(work).await.map_err(other)?
}

/// Writes `capture` as snapshot `meta` into `store` and adopts it.
fn store_capture(
    store: &Arc<dyn SnapshotStore>,
    meta: &SnapshotMetaOf<Config>,
    capture: Box<dyn MachineCapture>,
) -> io::Result<Stored> {
    let id = id_of(meta);
    let mut sink = store.create(id).map_err(other)?;
    write_head(&mut sink, meta)?;
    capture.write_to(&mut sink).map_err(other)?;
    let length = sink.finish().map_err(other)?;
    store.adopt(id).map_err(other)?;
    Ok(Stored::new(Arc::clone(store), id, length))
}

/// The newest snapshot, shared by the bridge and its builders.
pub(super) type Current = Arc<Mutex<Option<(SnapshotMetaOf<Config>, Stored)>>>;

/// Where a snapshot being built comes from.
pub(super) enum Source {
    /// The machine, captured at the applied position `meta` names.
    Machine(oneshot::Receiver<Result<Box<dyn MachineCapture>, ConsensusLogError>>),
    /// A snapshot another member took, already stored here.
    Offered(Stored),
}

/// A snapshot being built.
pub(crate) struct Capture {
    pub(super) meta: SnapshotMetaOf<Config>,
    pub(super) source: Option<Source>,
    pub(super) store: Arc<dyn SnapshotStore>,
    pub(super) current: Current,
}

impl RaftSnapshotBuilder<Config> for Capture {
    type SnapshotData = Stored;

    async fn build_snapshot(&mut self) -> Result<SnapshotOf<Config, Stored>, io::Error> {
        let meta = self.meta.clone();
        let stored = match self.source.take().ok_or_else(invalid)? {
            Source::Machine(receiver) => {
                let capture = receiver.await.map_err(other)?.map_err(other)?;
                let store = Arc::clone(&self.store);
                let meta = meta.clone();
                aside(move || store_capture(&store, &meta, capture)).await?
            }
            Source::Offered(stored) => {
                let adopted = stored.clone();
                aside(move || adopted.store.adopt(adopted.id).map_err(other)).await?;
                stored
            }
        };
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) =
            Some((meta.clone(), stored.clone()));
        Ok(Snapshot {
            meta,
            snapshot: stored,
        })
    }
}
