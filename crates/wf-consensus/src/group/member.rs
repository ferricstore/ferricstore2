//! One member of a replicated group (ADR-018), on the node's Tokio runtime:
//! its Raft over the consensus log, the peer listener, the proposer and a
//! watcher that demotes the machine when this member stops leading, and
//! the snapshots that let it drop its log behind them. A
//! member whose log lacks the mark that it votes asks the others what they
//! hold before it votes, and a fresh group is formed by its lowest-numbered
//! member once every other member reports an empty log (`join`).

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use openraft::type_config::async_runtime::watch::WatchReceiver;
use openraft::{Raft, SnapshotPolicy};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use wf_ports::consensus::log::{ConsensusLog, ReplicatedMachine, SnapshotStore};

use super::join::serve_member;
use super::machine::{Bridge, Taken};
use super::proposer::{self, Proposer};
use super::wire::{self, Recipient};
use super::{Config, Link, LogStore, Peers, peer};

/// How a member reaches the others.
pub struct MemberConfig {
    /// This member's id.
    pub id: u64,
    /// Every member's Raft address (`host:port`, resolved on each
    /// connection), this one's included.
    pub peers: BTreeMap<u64, String>,
    /// The bound listener for other members' Raft traffic.
    pub listener: std::net::TcpListener,
    /// Entries applied since the last snapshot at which a follower takes
    /// the next, and a leader keeps a follower's newer one (phase 3); 0
    /// takes none.
    pub snapshot_every: u64,
    /// Entries kept in the log behind a snapshot, so a slightly late member
    /// catches up from the log instead of a snapshot.
    pub keep_logs: u64,
}

/// A member that could not start.
#[derive(Debug)]
pub struct MemberError(String);

impl fmt::Display for MemberError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "group member: {}", self.0)
    }
}

impl std::error::Error for MemberError {}

fn failed(error: &impl fmt::Display) -> MemberError {
    MemberError(error.to_string())
}

/// A running member; [`Member::shutdown`] stops it.
pub struct Member {
    id: u64,
    raft: Raft<Config, Bridge>,
    runtime: Handle,
    tasks: Vec<JoinHandle<()>>,
}

/// A process's epoch: the wall clock in nanoseconds when it starts, so a
/// restarted process never takes an earlier one's.
fn epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |elapsed| {
            u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX)
        })
}

async fn watch(raft: Raft<Config, Bridge>, id: u64, machine: Arc<dyn ReplicatedMachine>) {
    let mut metrics = raft.metrics();
    let mut led = false;
    loop {
        let leads = metrics.borrow_watched().current_leader == Some(id);
        if led && !leads {
            machine.demote();
        }
        led = leads;
        if metrics.changed().await.is_err() {
            return;
        }
    }
}

/// Whether a member that applied entries up to `applied`, with its newest
/// snapshot at `taken`, takes the next one when it takes one every `every`.
const fn due(applied: u64, taken: u64, every: u64) -> bool {
    applied.saturating_sub(taken) >= every
}

/// The first newer snapshot a member `links` reaches holds: its last index
/// past `after` and at most `upto`.
async fn fetch(links: &mut [Link], after: u64, upto: u64) -> Option<Taken> {
    for link in links {
        if let Ok(Some(snapshot)) = peer::newest(link, after, upto).await {
            return Some(snapshot);
        }
    }
    None
}

/// Takes snapshots on member `own` of `peers` once `every` entries were
/// applied since its last one. A follower captures its machine; a
/// leader's shards are ahead of the log, so a leader keeps the newer
/// snapshot a follower took instead (one of entries it applied), and
/// `OpenRaft` drops its log behind it as a follower's. It asks over one
/// kept link per other member, so asking every few hundred milliseconds
/// opens no new connections.
async fn snapshots(
    (raft, bridge): (Raft<Config, Bridge>, Bridge),
    (peers, own): (BTreeMap<u64, String>, Recipient),
    every: u64,
) {
    if every == 0 {
        return;
    }
    let mut links: Vec<Link> = peers
        .into_iter()
        .filter(|(member, _)| *member != own.member)
        .map(|(member, address)| Link::to(address, Recipient { member, ..own }))
        .collect();
    // The last index fetched, so its build is not fetched again.
    let mut fetched = 0;
    loop {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let (leads, applied, taken) = {
            let metrics = raft.metrics();
            let metrics = metrics.borrow_watched();
            (
                metrics.current_leader == Some(own.member),
                metrics.last_applied.map_or(0, |log_id| log_id.index),
                metrics.snapshot.map_or(0, |log_id| log_id.index),
            )
        };
        if !due(applied, taken, every) {
            continue;
        }
        if leads {
            let Some(snapshot) = fetch(&mut links, taken.max(fetched), applied).await else {
                continue;
            };
            fetched = snapshot.meta.last_log_id.map_or(0, |log_id| log_id.index);
            bridge.offer(snapshot);
        }
        if raft.trigger().snapshot().await.is_err() {
            return;
        }
    }
}

impl Member {
    /// Starts the member on `runtime` over `log`, applying to `machine`;
    /// returns it with its proposer.
    ///
    /// # Errors
    /// Refuses a listener that cannot join the runtime and a Raft that does
    /// not start.
    pub fn start(
        runtime: &Handle,
        config: MemberConfig,
        (log, store): (Arc<dyn ConsensusLog>, Arc<dyn SnapshotStore>),
        machine: Arc<dyn ReplicatedMachine>,
    ) -> Result<(Self, Proposer), MemberError> {
        let MemberConfig {
            id,
            peers,
            listener,
            snapshot_every,
            keep_logs,
        } = config;
        let raft_config = openraft::Config {
            heartbeat_interval: 100,
            election_timeout_min: 500,
            election_timeout_max: 1_000,
            // Followers take snapshots themselves (`snapshots`).
            snapshot_policy: SnapshotPolicy::Never,
            max_in_snapshot_log_to_keep: keep_logs,
            // A member that lost its data comes back with an empty log; the
            // leader then starts its progress for it over instead of
            // stopping. OpenRaft warns that such a member may vote unsafely:
            // `ballot` keeps it from voting until it caught up, and the
            // leader never lowers its commit point (ADR-019).
            allow_log_reversion: Some(true),
            ..openraft::Config::default()
        }
        .validate()
        .map_err(|error| failed(&error))?;
        let bridge = Bridge::new(Arc::clone(&machine), (id, epoch()), store);
        runtime
            .block_on(bridge.restore())
            .map_err(|error| failed(&error))?;
        let raft = runtime
            .block_on(Raft::new(
                id,
                Arc::new(raft_config),
                Peers::new(peers.clone()),
                LogStore::new(Arc::clone(&log)),
                bridge.clone(),
            ))
            .map_err(|error| failed(&error))?;
        // Set once, before any entry can be applied with a leader to check.
        let _set = bridge.metrics.set(raft.metrics());
        listener
            .set_nonblocking(true)
            .map_err(|error| failed(&error))?;
        let listener = {
            let _entered = runtime.enter();
            tokio::net::TcpListener::from_std(listener).map_err(|error| failed(&error))?
        };
        let (proposer, proposing) = proposer::start(raft.clone(), bridge.epoch());
        let own = Recipient {
            group: wire::group_id(&peers),
            member: id,
        };
        let tasks = vec![
            runtime.spawn(snapshots(
                (raft.clone(), bridge),
                (peers.clone(), own),
                snapshot_every,
            )),
            runtime.spawn(serve_member(listener, raft.clone(), peers, id, log)),
            runtime.spawn(proposing),
            runtime.spawn(watch(raft.clone(), id, machine)),
        ];
        Ok((
            Self {
                id,
                raft,
                runtime: runtime.clone(),
                tasks,
            },
            proposer,
        ))
    }

    /// The member that leads, as this member knows it.
    #[must_use]
    pub fn leader(&self) -> Option<u64> {
        self.raft.metrics().borrow_watched().current_leader
    }

    /// The Raft term this member knows: it changes whenever leadership does.
    #[must_use]
    pub fn term(&self) -> u64 {
        self.raft.metrics().borrow_watched().current_term
    }

    /// Whether this member leads.
    #[must_use]
    pub fn leads(&self) -> bool {
        self.leader() == Some(self.id)
    }

    /// Stops the member: its Raft, listener, proposer and watcher.
    pub fn shutdown(self) {
        // A failed shutdown leaves nothing to clean up.
        let _stopped = self.runtime.block_on(self.raft.shutdown());
        for task in self.tasks {
            task.abort();
        }
    }
}
