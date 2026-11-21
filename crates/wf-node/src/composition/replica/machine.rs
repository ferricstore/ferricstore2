//! The node as its group's machine (ADR-018): committed rounds replayed into
//! the shards, promotion and demotion, and snapshots of the shards'
//! portable images.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use wf_application::commands::queue::ServiceError;
use wf_ports::consensus::log::{Applied, Captured, ConsensusLogError, ReplicatedMachine};

use crate::supervision::shard::Request;
use crate::supervision::shard::command::Logged;
use crate::supervision::shard::role::Role;
use crate::supervision::shard::sink;

use super::super::replay::by_shard;
use super::super::router::Router;

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
pub(super) struct Replicas {
    pub(super) router: Weak<Router>,
    pub(super) role: Role,
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

    fn snapshot(&self, done: Captured) -> bool {
        // A leader's shards hold rounds the log does not have yet.
        if self.role.serves() {
            return false;
        }
        match self.router.upgrade() {
            Some(router) => export(&router, done),
            None => done(Err(machine_error("the node is stopping"))),
        }
        true
    }

    fn install(&self, snapshot: Vec<u8>, done: Applied) {
        let Some(router) = self.router.upgrade() else {
            return done(Err(machine_error("the node is stopping")));
        };
        let Some(images) = split(&snapshot).filter(|images| images.len() == router.shards.len())
        else {
            return done(Err(machine_error(
                "the snapshot does not match this node's shards",
            )));
        };
        let Some(countdown) = Countdown::new(images.len(), done) else {
            return;
        };
        for (index, image) in images.into_iter().enumerate() {
            let part = countdown.clone();
            let done = Box::new(move |outcome| part.part(outcome));
            router.send(index, Request::Install { image, done });
        }
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

/// A snapshot's bytes: the shard count (`u16`), then each shard's image with
/// its length (`u32`).
fn join(images: &[Vec<u8>]) -> Option<Vec<u8>> {
    let mut out = u16::try_from(images.len()).ok()?.to_be_bytes().to_vec();
    for image in images {
        out.extend_from_slice(&u32::try_from(image.len()).ok()?.to_be_bytes());
        out.extend_from_slice(image);
    }
    Some(out)
}

fn split(snapshot: &[u8]) -> Option<Vec<Vec<u8>>> {
    let (count, mut rest) = snapshot.split_first_chunk::<2>()?;
    let mut images = Vec::new();
    for _ in 0..u16::from_be_bytes(*count) {
        let (length, after) = rest.split_first_chunk::<4>()?;
        let length = usize::try_from(u32::from_be_bytes(*length)).ok()?;
        let (image, after) = after.split_at_checked(length)?;
        images.push(image.to_vec());
        rest = after;
    }
    rest.is_empty().then_some(images)
}

/// Gathers every shard's portable image, in shard order.
fn export(router: &Router, done: Captured) {
    let shards = router.shards.len();
    let images: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(vec![Vec::new(); shards]));
    let gathered = Arc::clone(&images);
    let joined: Applied = Box::new(move |outcome| {
        let images = gathered.lock().unwrap_or_else(PoisonError::into_inner);
        let snapshot = join(&images).ok_or_else(|| machine_error("the snapshot is too large"));
        done(outcome.and(snapshot));
    });
    let Some(countdown) = Countdown::new(shards, joined) else {
        return;
    };
    for index in 0..shards {
        let part = countdown.clone();
        let images = Arc::clone(&images);
        let done = Box::new(move |image: Result<Vec<u8>, ServiceError>| {
            part.part(image.map(|bytes| keep(&images, index, bytes)));
        });
        router.send(index, Request::Export { done });
    }
}

fn keep(images: &Mutex<Vec<Vec<u8>>>, index: usize, bytes: Vec<u8>) {
    if let Some(slot) = images
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_mut(index)
    {
        *slot = bytes;
    }
}
