//! One member of a replicated group (ADR-018), on the node's Tokio runtime:
//! its Raft over the consensus log, the peer listener, the proposer and a
//! watcher that demotes the machine when this member stops leading. A fresh
//! group is formed by its lowest-numbered member.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use openraft::type_config::async_runtime::watch::WatchReceiver;
use openraft::{Raft, SnapshotPolicy};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use wf_ports::consensus::log::{ConsensusLog, ReplicatedMachine};

use super::machine::Bridge;
use super::proposer::{self, Proposer};
use super::{Config, LogStore, Peers, serve};

/// How a member reaches the others.
pub struct MemberConfig {
    /// This member's id.
    pub id: u64,
    /// Every member's Raft address, this one's included.
    pub peers: BTreeMap<u64, SocketAddr>,
    /// The bound listener for other members' Raft traffic.
    pub listener: std::net::TcpListener,
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
        log: Arc<dyn ConsensusLog>,
        machine: Arc<dyn ReplicatedMachine>,
    ) -> Result<(Self, Proposer), MemberError> {
        let MemberConfig {
            id,
            peers,
            listener,
        } = config;
        let raft_config = openraft::Config {
            heartbeat_interval: 100,
            election_timeout_min: 500,
            election_timeout_max: 1_000,
            snapshot_policy: SnapshotPolicy::Never,
            ..openraft::Config::default()
        }
        .validate()
        .map_err(|error| failed(&error))?;
        let fresh = log.bounds() == (None, None);
        let bridge = Bridge::new(Arc::clone(&machine), id, epoch());
        let raft = runtime
            .block_on(Raft::new(
                id,
                Arc::new(raft_config),
                Peers::<Vec<u8>>::new(peers.clone()),
                LogStore::new(log),
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
        let tasks = vec![
            runtime.spawn(serve(listener, raft.clone())),
            runtime.spawn(proposing),
            runtime.spawn(watch(raft.clone(), id, machine)),
        ];
        if fresh && peers.keys().next() == Some(&id) {
            let members: BTreeSet<u64> = peers.keys().copied().collect();
            // Another start of the same group may have formed it already.
            let _formed = runtime.block_on(raft.initialize(members));
        }
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
