//! A replicated node (ADR-018): one member of a group whose log is the
//! entry log. The node starts with empty shards; the group applies every
//! committed round through [`Replicas`], so a restarted member rebuilds its
//! state from the log. The leader's shards decide rounds and propose them;
//! a follower's shards apply the leader's committed rounds and refuse client
//! requests as not led here. A follower takes snapshots of its shards'
//! portable images; a member the leader's log no longer reaches installs
//! one, and a restarted member starts from its newest.

mod machine;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use wf_application::commands::queue::{Route, SLOTS};
use wf_consensus::group::{Member, MemberConfig};
use wf_ports::consensus::log::{ConsensusLog, SnapshotStore};
use wf_storage::log::EntryLog;
use wf_storage::snapshot::files::SnapshotFiles;

use crate::supervision;
use crate::supervision::shard::role::Role;
use crate::supervision::shard::sink::Sink;

use super::{NodeError, QueueNode, QueueNodeConfig, Started, cold, credit, start_schedules};

/// This node's place in its group.
pub struct Replica {
    /// This member's id.
    pub id: u64,
    /// Every member's Raft address (`host:port`, resolved on each
    /// connection), this one's included.
    pub peers: BTreeMap<u64, String>,
    /// The bound listener for other members' Raft traffic.
    pub listener: std::net::TcpListener,
    /// Entries applied since the last snapshot at which a follower takes
    /// the next; 0 takes none.
    pub snapshot_every: u64,
    /// Entries kept in the log behind a snapshot.
    pub keep_logs: u64,
}

/// The group member and the runtime it runs on.
pub(crate) struct Group {
    member: Option<Member>,
    runtime: Option<tokio::runtime::Runtime>,
}

impl Group {
    fn leader(&self) -> Option<u64> {
        self.member.as_ref().and_then(Member::leader)
    }

    /// The group's leadership as this member knows it.
    pub(crate) fn route(&self) -> Option<Route> {
        self.member.as_ref().map(|member| Route {
            leader: member.leader(),
            term: member.term(),
        })
    }

    /// Stops the member, then its runtime.
    pub(crate) fn stop(&mut self) {
        if let Some(member) = self.member.take() {
            member.shutdown();
        }
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl QueueNode {
    /// Opens member `replica.id` of a replicated group in `directory`.
    ///
    /// # Errors
    /// Refuses an invalid shard count, cold files or an entry log that do
    /// not open, and a member or thread that does not start.
    pub fn open_replicated(
        directory: &Path,
        config: QueueNodeConfig,
        replica: Replica,
    ) -> Result<Self, NodeError> {
        if config.shards == 0 || config.shards > SLOTS {
            return Err(NodeError::InvalidShards);
        }
        // Every committed round is applied again, so no earlier cold file
        // is named.
        let (cold, mut states) = cold::shards(directory, &config)?;
        cold::tidy(&cold, &mut states, &std::collections::HashSet::new())?;
        let log = EntryLog::open(&directory.join("group"), config.log)
            .map_err(|_| NodeError::Group("the entry log did not open"))?;
        let proposer = Arc::new(OnceLock::new());
        let role = Role::follower();
        let sink = Sink::Group {
            proposer: Arc::clone(&proposer),
            poisoned: Arc::new(AtomicBool::new(false)),
        };
        let started = Started::new(states, &config, sink, role.clone())?;
        let machine = Arc::new(machine::Replicas {
            router: Arc::downgrade(&started.router),
            role,
        });
        let runtime = supervision::runtime("wf-group").ok_or(NodeError::Thread)?;
        let member = MemberConfig {
            id: replica.id,
            peers: replica.peers,
            listener: replica.listener,
            snapshot_every: replica.snapshot_every,
            keep_logs: replica.keep_logs,
        };
        let snapshots = SnapshotFiles::open(&directory.join("group-snapshots"))
            .map_err(|_| NodeError::Group("the group's snapshot files did not open"))?;
        let stores = (
            Arc::new(log) as Arc<dyn ConsensusLog>,
            Arc::new(snapshots) as Arc<dyn SnapshotStore>,
        );
        let (member, proposing) = Member::start(runtime.handle(), member, stores, machine)
            .map_err(|_| NodeError::Group("the group member did not start"))?;
        let _set = proposer.set(proposing);
        let (schedule_jobs, scheduling) = start_schedules(&started.router, config.schedule_poll)?;
        Ok(Self {
            credit_cache: credit::Cache::default(),
            router: Some(started.router),
            waiters: started.waiters,
            shards: started.shards,
            completer: None,
            waiter: Some(started.waiter),
            snapshots: None,
            snapshot_stop: None,
            snapshotter: None,
            schedule_jobs: Some(schedule_jobs),
            scheduling: Some(scheduling),
            group: Some(Group {
                member: Some(member),
                runtime: Some(runtime),
            }),
        })
    }

    /// The member that leads this node's group, as this member knows it;
    /// `None` for a node that is not replicated.
    #[must_use]
    pub fn leader(&self) -> Option<u64> {
        self.group.as_ref().and_then(Group::leader)
    }

    /// Whether the node is ready for clients (ADR-019): not stopped by a
    /// lost round, and, in a group, knowing its leader (a follower then
    /// reroutes clients there).
    #[must_use]
    pub fn ready(&self) -> bool {
        !self.poisoned() && (self.group.is_none() || self.leader().is_some())
    }

    /// Whether a lost round stopped this node: it serves nothing more and
    /// must be reopened (ADR-018).
    #[must_use]
    pub fn poisoned(&self) -> bool {
        self.router
            .as_ref()
            .is_some_and(|router| router.poisoned.load(Ordering::Acquire))
    }
}
