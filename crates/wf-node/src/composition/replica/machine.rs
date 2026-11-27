//! The node as its group's machine (ADR-018): committed rounds replayed into
//! the shards, promotion and demotion, and snapshots of the shards'
//! portable images.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use wf_application::commands::queue::ServiceError;

use wf_ports::consensus::log::{
    Applied, Capture, Captured, ConsensusLogError, ReplicatedMachine, SnapshotSource,
};

use crate::supervision::shard::command::Logged;
use crate::supervision::shard::role::Role;
use crate::supervision::shard::sink;
use crate::supervision::shard::{Part as Fed, Request};

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
            Some(router) => done(Ok(Box::new(export(&router)))),
            None => done(Err(machine_error("the node is stopping"))),
        }
        true
    }

    fn install(&self, snapshot: Box<dyn SnapshotSource>, done: Applied) {
        let Some(router) = self.router.upgrade() else {
            return done(Err(machine_error("the node is stopping")));
        };
        done(install(&router, snapshot));
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

/// Blocks a shard sends ahead of the snapshot writer, or an installing
/// shard is sent ahead of its reading.
const BLOCKS_AHEAD: usize = 16;

/// A shard's export: its blocks as they come, then its outcome.
type Part = (Receiver<Vec<u8>>, Receiver<Result<(), ServiceError>>);

/// Each shard's export, in shard order.
struct Exported {
    parts: Vec<Part>,
}

/// Starts every shard's export at once, so each images the state every
/// proposal applied so far left; a shard waits while the writer, reading
/// the shards in order, catches up with it.
fn export(router: &Router) -> Exported {
    let parts = (0..router.shards.len())
        .map(|index| {
            let (blocks, arriving) = mpsc::sync_channel(BLOCKS_AHEAD);
            let (outcome, answered) = mpsc::channel();
            let done = Box::new(move |result| {
                // The writer may have stopped waiting.
                drop(outcome.send(result));
            });
            router.send(index, Request::Export { blocks, done });
            (arriving, answered)
        })
        .collect();
    Exported { parts }
}

fn written(error: &std::io::Error) -> ConsensusLogError {
    machine_error(&format!("the snapshot could not be written: {error}"))
}

/// A snapshot's machine bytes: the shard count (`u16`), then each shard's
/// blocks in shard order, each a `u32` length and its bytes.
impl Capture for Exported {
    fn write_to(self: Box<Self>, out: &mut dyn Write) -> Result<(), ConsensusLogError> {
        let count = u16::try_from(self.parts.len())
            .map_err(|_| machine_error("too many shards for a snapshot"))?;
        out.write_all(&count.to_be_bytes())
            .map_err(|error| written(&error))?;
        for (arriving, answered) in self.parts {
            for block in arriving {
                let length = u32::try_from(block.len())
                    .map_err(|_| machine_error("a snapshot block is too large"))?;
                out.write_all(&length.to_be_bytes())
                    .and_then(|()| out.write_all(&block))
                    .map_err(|error| written(&error))?;
            }
            answered
                .recv()
                .map_err(|_| machine_error("a shard did not answer its export"))?
                .map_err(|error| machine_error(&error.to_string()))?;
        }
        Ok(())
    }
}

fn read_block(source: &mut dyn Read) -> std::io::Result<Vec<u8>> {
    let mut length = [0; 4];
    source.read_exact(&mut length)?;
    let length = usize::try_from(u32::from_be_bytes(length)).map_err(std::io::Error::other)?;
    let mut block = vec![0; length];
    source.read_exact(&mut block)?;
    Ok(block)
}

/// Reads one shard's blocks from `source` and passes them to it: its
/// head, its records up to the empty block, then its trailer.
fn feed(source: &mut dyn Read, shard: &SyncSender<Fed>) -> std::io::Result<()> {
    let stopped = |_| std::io::Error::other("an installing shard stopped");
    let head = read_block(source)?;
    shard.send(Fed::Block(head)).map_err(stopped)?;
    loop {
        let block = read_block(source)?;
        let last = block.is_empty();
        shard.send(Fed::Block(block)).map_err(stopped)?;
        if last {
            break;
        }
    }
    let trailer = read_block(source)?;
    shard.send(Fed::Block(trailer)).map_err(stopped)
}

/// Installs `source` into every shard, reading it once: each shard builds
/// its new state from its blocks as they are read, and takes it once the
/// whole snapshot was read and its check held (a read that fails, the
/// check included, leaves every shard as it was).
fn install(router: &Router, mut source: Box<dyn SnapshotSource>) -> Result<(), ConsensusLogError> {
    let mut count = [0; 2];
    source
        .read_exact(&mut count)
        .map_err(|error| written(&error))?;
    if usize::from(u16::from_be_bytes(count)) != router.shards.len() {
        return Err(machine_error(
            "the snapshot does not match this node's shards",
        ));
    }
    let mut shards = Vec::new();
    for index in 0..router.shards.len() {
        let (blocks, arriving) = mpsc::sync_channel(BLOCKS_AHEAD);
        let (outcome, answered) = mpsc::channel();
        let done = Box::new(move |result| {
            // The installer may have stopped waiting.
            drop(outcome.send(result));
        });
        router.send(
            index,
            Request::Install {
                blocks: arriving,
                done,
            },
        );
        shards.push((blocks, answered));
    }
    let read = shards
        .iter()
        .try_for_each(|(blocks, _)| feed(&mut source, blocks))
        .and_then(|()| {
            let mut after = [0; 1];
            match source.read(&mut after)? {
                0 => Ok(()),
                _ => Err(std::io::Error::other(
                    "bytes after the snapshot's last shard",
                )),
            }
        });
    let mut outcome = read.map_err(|error| written(&error));
    for (blocks, answered) in shards {
        if outcome.is_ok() && blocks.send(Fed::Commit).is_err() {
            outcome = Err(machine_error("an installing shard stopped"));
        }
        // Dropped without a commit, a shard keeps its state.
        drop(blocks);
        let installed = answered.recv().ok().and_then(Result::ok);
        if outcome.is_ok() && installed.is_none() {
            outcome = Err(machine_error("a shard did not install its image"));
        }
    }
    outcome
}
