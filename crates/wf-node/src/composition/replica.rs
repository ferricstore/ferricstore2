//! A replicated node (ADR-018): one member of a group whose log is the
//! entry log. The node starts with empty shards; the group applies every
//! committed round through [`Replicas`], so a restarted member rebuilds its
//! state from the log. The leader's shards decide rounds and propose them;
//! a follower's shards apply the leader's committed rounds and refuse client
//! requests as not led here. Snapshots wait for phase 3.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

use wf_application::commands::queue::{Route, SLOTS, ServiceError};
use wf_consensus::group::{Member, MemberConfig};
use wf_ports::consensus::log::{Applied, ConsensusLogError, ReplicatedMachine};
use wf_storage::log::EntryLog;

use crate::supervision;
use crate::supervision::shard::Request;
use crate::supervision::shard::command::Logged;
use crate::supervision::shard::role::Role;
use crate::supervision::shard::sink::{self, Sink};

use super::replay::by_shard;
use super::router::Router;
use super::{NodeError, QueueNode, QueueNodeConfig, Started, cold, credit, start_schedules};

/// This node's place in its group.
pub struct Replica {
    /// This member's id.
    pub id: u64,
    /// Every member's Raft address, this one's included.
    pub peers: BTreeMap<u64, SocketAddr>,
    /// The bound listener for other members' Raft traffic.
    pub listener: std::net::TcpListener,
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
        cold::tidy(&cold, &mut states)?;
        let log = EntryLog::open(&directory.join("group"), config.log)
            .map_err(|_| NodeError::Group("the entry log did not open"))?;
        let proposer = Arc::new(OnceLock::new());
        let role = Role::follower();
        let sink = Sink::Group {
            proposer: Arc::clone(&proposer),
            poisoned: Arc::new(AtomicBool::new(false)),
        };
        let started = Started::new(states, &config, sink, role.clone())?;
        let machine = Arc::new(Replicas {
            router: Arc::downgrade(&started.router),
            role,
        });
        let runtime = supervision::runtime("wf-group").ok_or(NodeError::Thread)?;
        let member = MemberConfig {
            id: replica.id,
            peers: replica.peers,
            listener: replica.listener,
        };
        let (member, proposing) =
            Member::start(runtime.handle(), member, Arc::new(log), machine)
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

    /// Whether a lost round stopped this node: it serves nothing more and
    /// must be reopened (ADR-018).
    #[must_use]
    pub fn poisoned(&self) -> bool {
        self.router
            .as_ref()
            .is_some_and(|router| router.poisoned.load(Ordering::Acquire))
    }
}

fn machine_error(problem: &str) -> ConsensusLogError {
    ConsensusLogError::new(problem.to_owned())
}

/// What a countdown waits on: the answers left, the first failure and the
/// call to make.
struct Waiting {
    left: AtomicUsize,
    failure: Mutex<Option<ConsensusLogError>>,
    done: Mutex<Option<Applied>>,
}

/// Joins the shards' answers to one machine call.
#[derive(Clone)]
struct Countdown(Arc<Waiting>);

impl Countdown {
    /// Waits for `parts` answers; with none, `done` is called at once.
    fn new(parts: usize, done: Applied) -> Option<Self> {
        if parts == 0 {
            done(Ok(()));
            return None;
        }
        Some(Self(Arc::new(Waiting {
            left: AtomicUsize::new(parts),
            failure: Mutex::new(None),
            done: Mutex::new(Some(done)),
        })))
    }

    fn part(&self, outcome: Result<(), ServiceError>) {
        let Waiting {
            left,
            failure,
            done,
        } = &*self.0;
        if let Err(error) = outcome {
            *failure.lock().unwrap_or_else(PoisonError::into_inner) =
                Some(machine_error(&error.to_string()));
        }
        if left.fetch_sub(1, Ordering::AcqRel) != 1 {
            return;
        }
        let finished = done.lock().unwrap_or_else(PoisonError::into_inner).take();
        let failure = failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(done) = finished {
            done(failure.map_or(Ok(()), Err));
        }
    }
}

/// Leads once every shard serves, then reports.
fn promoted(role: Role, done: Applied) -> Applied {
    Box::new(move |outcome| {
        if outcome.is_ok() {
            role.lead();
        }
        done(outcome);
    })
}

/// The node as its group's machine.
struct Replicas {
    router: Weak<Router>,
    role: Role,
}

/// A committed round's changes, by shard.
fn changes(router: &Router, proposals: &[Arc<[u8]>]) -> Option<Vec<Vec<Logged>>> {
    let mut per_shard: Vec<Vec<Logged>> = (0..router.shards.len()).map(|_| Vec::new()).collect();
    for proposal in proposals {
        for record in sink::records(proposal)? {
            for (index, change) in by_shard(Logged::decode(&record)?, router.shard_count)? {
                per_shard.get_mut(index)?.push(change);
            }
        }
    }
    Some(per_shard)
}

impl ReplicatedMachine for Replicas {
    fn apply(&self, proposals: Vec<Arc<[u8]>>, done: Applied) {
        let Some(router) = self.router.upgrade() else {
            return done(Err(machine_error("the node is stopping")));
        };
        let Some(per_shard) = changes(&router, &proposals) else {
            return done(Err(machine_error("a committed round does not decode")));
        };
        let parts = per_shard
            .iter()
            .filter(|changes| !changes.is_empty())
            .count();
        let Some(countdown) = Countdown::new(parts, done) else {
            return;
        };
        for (index, changes) in per_shard.into_iter().enumerate() {
            if changes.is_empty() {
                continue;
            }
            let part = countdown.clone();
            let done = Box::new(move |outcome| part.part(outcome));
            router.send(index, Request::Replay { changes, done });
        }
    }

    fn promote(&self, done: Applied) {
        let Some(router) = self.router.upgrade() else {
            return done(Err(machine_error("the node is stopping")));
        };
        let weak = Weak::clone(&self.router);
        let role = self.role.clone();
        let policies = Box::new(move |policies: Result<Vec<_>, ServiceError>| {
            let (Ok(policies), Some(router)) = (policies, weak.upgrade()) else {
                return done(Err(machine_error("shard 0 did not hand over its policies")));
            };
            let promoted = promoted(role, done);
            let Some(countdown) = Countdown::new(router.shards.len(), promoted) else {
                return;
            };
            for index in 0..router.shards.len() {
                let part = countdown.clone();
                let done = Box::new(move |outcome| part.part(outcome));
                let policies = policies.clone();
                router.send(index, Request::Live { policies, done });
            }
        });
        router.send(0, Request::Policies { done: policies });
    }

    fn demote(&self) {
        self.role.follow();
        if let Some(router) = self.router.upgrade() {
            for index in 0..router.shards.len() {
                router.send(
                    index,
                    Request::Quiet {
                        done: Box::new(drop),
                    },
                );
            }
        }
    }
}
