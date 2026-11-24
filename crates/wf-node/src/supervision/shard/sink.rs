//! Where a shard round's changes go (ADR-018): the node's own group log,
//! whose completer releases the round's replies after the sync, or a
//! replicated group's proposer, which releases them after the commit. A
//! round without changes waits for the shard's earlier rounds either way.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, OnceLock};

use wf_application::commands::queue::ServiceError;
use wf_consensus::group::Proposer;
use wf_storage::log::GroupLog;

use super::super::completer::{Completion, Finish};
use super::command::{self, Logged};

/// A shard's sink.
#[derive(Clone)]
pub(crate) enum Sink {
    /// The node's own log.
    Local {
        log: Arc<GroupLog>,
        completions: Sender<Completion>,
        /// The most bytes one append, and so one record, takes.
        max_append_bytes: usize,
    },
    /// A replicated group's proposer, and whether a lost round stopped the
    /// node (fail-stop: its shards hold changes the group never took).
    Group {
        /// Set once the group's member started.
        proposer: Arc<OnceLock<Proposer>>,
        poisoned: Arc<AtomicBool>,
    },
}

/// A round handed to the sink and not yet released.
pub(crate) enum Pending {
    /// Appended up to this log sequence.
    Appended(u64),
    /// Encoded for the group; empty when it changed nothing.
    Proposed(Vec<u8>),
}

/// Appends the round's changes in chunks that fit one log block; returns the
/// sequence covering all of them, or `None` when an append failed.
fn append(log: &GroupLog, logged: &[Logged], max_bytes: usize, previous: u64) -> Option<u64> {
    let mut sequence = previous;
    let mut chunk: Vec<Vec<u8>> = Vec::new();
    let mut chunk_bytes = 0_usize;
    for record in command::records(logged, max_bytes)? {
        let size = record.len().saturating_add(4);
        if !chunk.is_empty() && chunk_bytes.saturating_add(size) > max_bytes {
            sequence = log.append_batch(std::mem::take(&mut chunk)).ok()?;
            chunk_bytes = 0;
        }
        chunk_bytes = chunk_bytes.saturating_add(size);
        chunk.push(record);
    }
    if !chunk.is_empty() {
        sequence = log.append_batch(chunk).ok()?;
    }
    Some(sequence)
}

/// A round's records as one proposal: each a `u32` length and its bytes;
/// empty for a round without changes. A run of after-images sharing a
/// payload is one record however long: the proposal holds the round
/// whole either way.
pub(crate) fn proposal(logged: &[Logged]) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    for record in command::records(logged, usize::MAX)? {
        body.extend_from_slice(&u32::try_from(record.len()).ok()?.to_be_bytes());
        body.extend_from_slice(&record);
    }
    Some(body)
}

/// A proposal's records.
pub(crate) fn records(mut body: &[u8]) -> Option<Vec<Vec<u8>>> {
    let mut records = Vec::new();
    while let Some((length, rest)) = body.split_first_chunk::<4>() {
        let length = usize::try_from(u32::from_be_bytes(*length)).ok()?;
        let record = rest.get(..length)?;
        records.push(record.to_vec());
        body = rest.get(length..)?;
    }
    body.is_empty().then_some(records)
}

impl Sink {
    /// Takes the round's changes; `None` when the sink failed for good.
    pub(crate) fn take(&self, logged: &[Logged], previous: u64) -> Option<Pending> {
        match self {
            Self::Local {
                log,
                max_append_bytes,
                ..
            } => append(log, logged, *max_append_bytes, previous).map(Pending::Appended),
            Self::Group { .. } => proposal(logged).map(Pending::Proposed),
        }
    }

    /// Releases the round's replies once its changes are durable.
    pub(crate) fn release(&self, shard: u16, pending: Pending, finishes: Vec<Finish>) {
        match (self, pending) {
            (Self::Local { completions, .. }, Pending::Appended(sequence)) => {
                release_local(completions, sequence, finishes);
            }
            (Self::Group { proposer, poisoned }, Pending::Proposed(body)) => {
                release_group((proposer, poisoned), shard, body, finishes);
            }
            (_, _) => finish_all(finishes, &Err(ServiceError::Unavailable)),
        }
    }
}

fn finish_all(finishes: Vec<Finish>, outcome: &Result<(), ServiceError>) {
    for finish in finishes {
        finish(outcome.clone());
    }
}

fn release_local(completions: &Sender<Completion>, sequence: u64, finishes: Vec<Finish>) {
    if finishes.is_empty() {
        return;
    }
    if let Err(returned) = completions.send(Completion { sequence, finishes }) {
        finish_all(returned.0.finishes, &Err(ServiceError::Unavailable));
    }
}

/// Replies after the commit; a round the group did not take stops the
/// node (fail-stop, ADR-018).
fn committed(poisoned: &AtomicBool, finishes: Vec<Finish>, committed: bool) {
    if !committed {
        poisoned.store(true, Ordering::Release);
    }
    let outcome = if committed {
        Ok(())
    } else {
        Err(ServiceError::Unavailable)
    };
    finish_all(finishes, &outcome);
}

fn release_group(
    (proposer, poisoned): (&OnceLock<Proposer>, &Arc<AtomicBool>),
    shard: u16,
    body: Vec<u8>,
    finishes: Vec<Finish>,
) {
    if finishes.is_empty() && body.is_empty() {
        return;
    }
    let Some(proposer) = proposer.get() else {
        return finish_all(finishes, &Err(ServiceError::Unavailable));
    };
    let poisoned = Arc::clone(poisoned);
    proposer.propose(
        shard,
        (!body.is_empty()).then_some(body),
        Box::new(move |done| committed(&poisoned, finishes, done)),
    );
}
