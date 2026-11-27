//! The state machine bridge (ADR-018): `OpenRaft` applies committed entries
//! here, and the node's machine (the `ReplicatedMachine` port) sees what it
//! must change.
//!
//! A proposal carries the epoch of the process that proposed it; a leader's
//! shards applied its own rounds when they decided them, so entries with
//! this process's epoch only advance the applied position. Other proposals
//! go to the machine in commit order. The blank entry a new leader commits
//! promotes the machine once it is this member's, in the term it leads, so
//! every earlier entry was applied first. A follower's machine is captured
//! for snapshots (phase 3); a leader's is not, since its shards hold rounds
//! not yet committed: a leader keeps a follower's snapshot it was offered
//! instead, so it can drop its log behind it.

#[cfg(test)]
mod tests;

use std::future::{Future, ready};
use std::io;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use futures_util::TryStreamExt;
use futures_util::stream::Stream;
use openraft::storage::{EntryResponder, RaftStateMachine};
use openraft::type_config::alias::{
    EntryOf, LogIdOf, SnapshotMetaOf, SnapshotOf, StoredMembershipOf, WatchReceiverOf,
};

/// A snapshot as `OpenRaft` hands it around.
pub(super) type Taken = SnapshotOf<Config, Vec<u8>>;
use openraft::type_config::async_runtime::watch::WatchReceiver;
use openraft::{EntryPayload, OptionalSend, RaftMetrics, Snapshot, SnapshotMeta, StoredMembership};
use tokio::sync::oneshot;
use wf_ports::consensus::log::{Applied, ReplicatedMachine, SnapshotStore};

use super::Config;
use super::snapshot::{self, Capture, Current};

/// Bytes before a proposal's body: the proposing process's epoch.
const EPOCH_BYTES: usize = 8;

/// A proposal's bytes: `epoch`, then `body`.
pub(super) fn wrap(epoch: u64, body: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(body.len().saturating_add(EPOCH_BYTES));
    bytes.extend_from_slice(&epoch.to_be_bytes());
    bytes.extend_from_slice(body);
    bytes
}

fn split(bytes: &[u8]) -> Option<(u64, &[u8])> {
    let (epoch, body) = bytes.split_first_chunk::<EPOCH_BYTES>()?;
    Some((u64::from_be_bytes(*epoch), body))
}

fn failed(error: &impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

type Position = (Option<LogIdOf<Config>>, StoredMembershipOf<Config>);

/// The bridge; clones share their state.
#[derive(Clone)]
pub(crate) struct Bridge {
    machine: Arc<dyn ReplicatedMachine>,
    id: u64,
    epoch: u64,
    position: Arc<Mutex<Position>>,
    store: Arc<dyn SnapshotStore>,
    current: Current,
    /// A follower's snapshot offered to this member while it leads.
    offered: Arc<Mutex<Option<Taken>>>,
    /// The member's metrics, once its Raft runs.
    pub(super) metrics: Arc<OnceLock<WatchReceiverOf<Config, RaftMetrics<Config>>>>,
}

/// Waits for a machine call that reports through `Applied`.
async fn machine_call(call: impl FnOnce(Applied)) -> io::Result<()> {
    let (sender, receiver) = oneshot::channel();
    call(Box::new(move |outcome| {
        // The apply loop may have stopped waiting; nothing to report then.
        let _sent = sender.send(outcome);
    }));
    receiver
        .await
        .map_err(|error| failed(&error))?
        .map_err(|error| failed(&error))
}

impl Bridge {
    pub(crate) fn new(
        machine: Arc<dyn ReplicatedMachine>,
        (id, epoch): (u64, u64),
        store: Arc<dyn SnapshotStore>,
    ) -> Self {
        Self {
            machine,
            id,
            epoch,
            position: Arc::new(Mutex::new((None, StoredMembership::default()))),
            store,
            current: Arc::default(),
            offered: Arc::default(),
            metrics: Arc::new(OnceLock::new()),
        }
    }

    /// Installs the newest stored snapshot, if any, before the member
    /// starts: the machine and the applied position become its.
    pub(super) async fn restore(&self) -> io::Result<()> {
        let Some((meta, data)) = snapshot::newest(self.store.as_ref())? else {
            return Ok(());
        };
        self.install(meta, data, false).await
    }

    async fn install(
        &self,
        meta: SnapshotMetaOf<Config>,
        data: Vec<u8>,
        keep: bool,
    ) -> io::Result<()> {
        let bytes = data.clone();
        machine_call(|done| self.machine.install(bytes, done)).await?;
        let data = Arc::new(data);
        if keep {
            snapshot::save_aside(Arc::clone(&self.store), meta.clone(), Arc::clone(&data)).await?;
        }
        *self.position.lock().unwrap_or_else(PoisonError::into_inner) =
            (meta.last_log_id, meta.last_membership.clone());
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = Some((meta, data));
        Ok(())
    }

    /// Offers `snapshot`, which another member took of committed entries
    /// this member applied, for this member to keep while it leads.
    pub(super) fn offer(&self, snapshot: Taken) {
        *self.offered.lock().unwrap_or_else(PoisonError::into_inner) = Some(snapshot);
    }

    /// Whether `meta` is past the newest snapshot this member holds: an
    /// older one would leave it sending a snapshot its log no longer
    /// follows.
    fn newer(&self, meta: &SnapshotMetaOf<Config>) -> bool {
        let held = self
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .and_then(|(held, _)| held.last_log_id.map(|log_id| log_id.index));
        meta.last_log_id.map(|log_id| log_id.index) > held
    }

    /// A builder of a snapshot at the applied position; while the machine
    /// serves as the leader's, one of the snapshot offered, if it is newer
    /// than this member's.
    fn capture(&self) -> Option<Capture> {
        let offered = self
            .offered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let (sender, receiver) = oneshot::channel();
        let started = self.machine.snapshot(Box::new(move |captured| {
            // The builder may have been dropped; nothing waits then.
            let _sent = sender.send(captured);
        }));
        let (meta, receiver) = if started {
            let (last_log_id, last_membership) = self.position();
            let meta = SnapshotMeta {
                last_log_id,
                last_membership,
            };
            (meta, receiver)
        } else {
            let Snapshot { meta, snapshot } =
                offered.filter(|offered| self.newer(&offered.meta))?;
            let (sender, receiver) = oneshot::channel();
            let _sent = sender.send(Ok(snapshot));
            (meta, receiver)
        };
        Some(Capture {
            meta,
            bytes: Some(receiver),
            store: Arc::clone(&self.store),
            current: Arc::clone(&self.current),
        })
    }

    pub(super) const fn epoch(&self) -> u64 {
        self.epoch
    }

    fn position(&self) -> Position {
        self.position
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Whether this member leads in `term` now.
    fn leads_in(&self, term: u64) -> bool {
        self.metrics.get().is_some_and(|metrics| {
            let metrics = metrics.borrow_watched();
            metrics.current_leader == Some(self.id) && metrics.current_term == term
        })
    }

    async fn flush(&self, pending: &mut Vec<Arc<[u8]>>) -> io::Result<()> {
        if pending.is_empty() {
            return Ok(());
        }
        let proposals = std::mem::take(pending);
        machine_call(|done| self.machine.apply(proposals, done)).await
    }
}

impl Bridge {
    /// Takes one committed entry: other processes' proposals wait in
    /// `pending` for the machine; this member's blank entry promotes it.
    async fn take(&self, entry: EntryOf<Config>, pending: &mut Vec<Arc<[u8]>>) -> io::Result<()> {
        match entry.payload {
            EntryPayload::Normal(proposal) => {
                let (epoch, body) =
                    split(&proposal.0).ok_or_else(|| failed(&"malformed proposal"))?;
                if epoch != self.epoch {
                    pending.push(body.into());
                }
            }
            EntryPayload::Blank => {
                let leader = entry.log_id.leader_id;
                if leader.node_id == self.id && self.leads_in(leader.term) {
                    self.flush(pending).await?;
                    machine_call(|done| self.machine.promote(done)).await?;
                }
            }
            EntryPayload::Membership(membership) => {
                self.position
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .1 = StoredMembership::new(Some(entry.log_id), membership);
            }
        }
        self.position
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .0 = Some(entry.log_id);
        Ok(())
    }
}

impl RaftStateMachine<Config> for Bridge {
    type SnapshotData = Vec<u8>;
    type SnapshotBuilder = Capture;

    fn applied_state(&mut self) -> impl Future<Output = io::Result<Position>> + Send {
        ready(Ok(self.position()))
    }

    async fn apply<Strm>(&mut self, mut entries: Strm) -> Result<(), io::Error>
    where
        Strm: Stream<Item = Result<EntryResponder<Config>, io::Error>> + Unpin + OptionalSend,
    {
        let mut pending: Vec<Arc<[u8]>> = Vec::new();
        while let Some((entry, responder)) = entries.try_next().await? {
            self.take(entry, &mut pending).await?;
            if let Some(responder) = responder {
                responder.send(());
            }
        }
        self.flush(&mut pending).await
    }

    fn try_create_snapshot_builder(
        &mut self,
        _force: bool,
    ) -> impl Future<Output = Option<Capture>> + Send {
        ready(self.capture())
    }

    fn get_snapshot_builder(&mut self) -> impl Future<Output = Capture> + Send {
        // `OpenRaft` asks through `try_create_snapshot_builder`; this one has
        // no way to decline, so a leader's builder fails instead.
        let capture = self.capture().unwrap_or_else(|| Capture {
            meta: SnapshotMeta::default(),
            bytes: None,
            store: Arc::clone(&self.store),
            current: Arc::clone(&self.current),
        });
        ready(capture)
    }

    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMetaOf<Config>,
        snapshot: Vec<u8>,
    ) -> io::Result<()> {
        self.install(meta.clone(), snapshot, true).await
    }

    fn get_current_snapshot(
        &mut self,
    ) -> impl Future<Output = io::Result<Option<SnapshotOf<Config, Vec<u8>>>>> + Send {
        let current = self
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        ready(Ok(current.map(|(meta, data)| Snapshot {
            meta,
            snapshot: data.as_ref().clone(),
        })))
    }
}
