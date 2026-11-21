//! The state machine bridge (ADR-018): `OpenRaft` applies committed entries
//! here, and the node's machine (the `ReplicatedMachine` port) sees what it
//! must change.
//!
//! A proposal carries the epoch of the process that proposed it; a leader's
//! shards applied its own rounds when they decided them, so entries with
//! this process's epoch only advance the applied position. Other proposals
//! go to the machine in commit order. The blank entry a new leader commits
//! promotes the machine once it is this member's, in the term it leads, so
//! every earlier entry was applied first. Snapshots are refused until
//! phase 3.

use std::future::{Future, ready};
use std::io;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use futures_util::TryStreamExt;
use futures_util::stream::Stream;
use openraft::storage::{EntryResponder, RaftSnapshotBuilder, RaftStateMachine};
use openraft::type_config::alias::{
    EntryOf, LogIdOf, SnapshotMetaOf, SnapshotOf, StoredMembershipOf, WatchReceiverOf,
};
use openraft::type_config::async_runtime::watch::WatchReceiver;
use openraft::{EntryPayload, OptionalSend, RaftMetrics, StoredMembership};
use tokio::sync::oneshot;
use wf_ports::consensus::log::{Applied, ReplicatedMachine};

use super::Config;

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
    pub(crate) fn new(machine: Arc<dyn ReplicatedMachine>, id: u64, epoch: u64) -> Self {
        Self {
            machine,
            id,
            epoch,
            position: Arc::new(Mutex::new((None, StoredMembership::default()))),
            metrics: Arc::new(OnceLock::new()),
        }
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

impl RaftSnapshotBuilder<Config> for Bridge {
    type SnapshotData = Vec<u8>;

    fn build_snapshot(
        &mut self,
    ) -> impl Future<Output = Result<SnapshotOf<Config, Vec<u8>>, io::Error>> + Send {
        ready(Err(failed(&"group snapshots are not built yet")))
    }
}

impl RaftStateMachine<Config> for Bridge {
    type SnapshotData = Vec<u8>;
    type SnapshotBuilder = Self;

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

    fn get_snapshot_builder(&mut self) -> impl Future<Output = Self> + Send {
        ready(self.clone())
    }

    fn install_snapshot(
        &mut self,
        _meta: &SnapshotMetaOf<Config>,
        _snapshot: Vec<u8>,
    ) -> impl Future<Output = io::Result<()>> + Send {
        ready(Err(failed(&"group snapshots are not installed yet")))
    }

    fn get_current_snapshot(
        &mut self,
    ) -> impl Future<Output = io::Result<Option<SnapshotOf<Config, Vec<u8>>>>> + Send {
        ready(Ok(None))
    }
}
