//! The entry log (ADR-018): a consensus log of identified entries on the
//! group log, without consensus types.
//!
//! Every change is a group log record, so the log stays append-only: an
//! entry with its id, a truncation that drops the entries after an index (a
//! follower overwriting an uncommitted suffix), a vote, and a purge that
//! drops the entries up to an id. Opening replays them in order. Appends
//! report durability through a completion called by the notifier thread;
//! truncations, votes and purges return once durable. A purge also removes
//! the segments before the oldest kept entry, after writing the vote and the
//! purge again so they outlive those segments. Entries since the last purge
//! are kept in memory for readers.

mod codec;
mod notifier;

use std::collections::VecDeque;
use std::fmt;
use std::ops::Range;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::group::{DirectoryStorage, GroupLog, GroupLogError, GroupLogLimits, SegmentStorage};
use codec::Record;
pub use notifier::Durable;
use notifier::Notifier;
pub use wf_ports::consensus::log::EntryId;
use wf_ports::consensus::log::{ConsensusLog, ConsensusLogError};

/// A refused open or change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryLogError {
    /// The group log refused it.
    Log(GroupLogError),
    /// A record at this position is not an entry log record.
    Malformed {
        /// Position among the recovered records.
        record: usize,
    },
    /// An entry does not follow the one before it.
    Gap {
        /// The index that did not follow.
        index: u64,
    },
}

impl fmt::Display for EntryLogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Log(error) => write!(formatter, "entry log: {error}"),
            Self::Malformed { record } => {
                write!(formatter, "entry log record {record} is malformed")
            }
            Self::Gap { index } => write!(formatter, "entry log index {index} leaves a gap"),
        }
    }
}

impl std::error::Error for EntryLogError {}

impl From<GroupLogError> for EntryLogError {
    fn from(error: GroupLogError) -> Self {
        Self::Log(error)
    }
}

struct Kept {
    id: EntryId,
    bytes: Arc<[u8]>,
    /// The segment holding it, or an earlier one.
    segment: u64,
}

#[derive(Default)]
struct State {
    entries: VecDeque<Kept>,
    purged: Option<EntryId>,
    vote: Option<Vec<u8>>,
}

impl State {
    fn last(&self) -> Option<EntryId> {
        self.entries.back().map(|kept| kept.id).or(self.purged)
    }

    fn truncate(&mut self, after: Option<u64>) {
        self.entries
            .retain(|kept| after.is_some_and(|after| kept.id.index <= after));
    }

    /// Adds an entry after the last, replacing any from its index on.
    fn push(&mut self, id: EntryId, bytes: Arc<[u8]>, segment: u64) -> Result<(), EntryLogError> {
        self.truncate(id.index.checked_sub(1));
        let expected = self
            .last()
            .map_or(Some(id.index), |last| last.index.checked_add(1));
        if expected != Some(id.index) {
            return Err(EntryLogError::Gap { index: id.index });
        }
        self.entries.push_back(Kept { id, bytes, segment });
        Ok(())
    }

    fn purge(&mut self, upto: EntryId) {
        self.entries.retain(|kept| kept.id.index > upto.index);
        if self.purged.is_none_or(|purged| purged.index < upto.index) {
            self.purged = Some(upto);
        }
    }
}

/// The open entry log.
pub struct EntryLog {
    notifier: Notifier,
    state: Mutex<State>,
    log: Arc<GroupLog>,
}

fn segment_of(segments: &[u64], starts: &[usize], position: usize) -> u64 {
    segments
        .iter()
        .zip(starts)
        .take_while(|(_, start)| **start <= position)
        .last()
        .map_or(0, |(segment, _)| *segment)
}

impl EntryLog {
    /// Opens or creates the entry log in `directory`.
    ///
    /// # Errors
    /// As [`Self::open_with`].
    pub fn open(directory: &Path, limits: GroupLogLimits) -> Result<Self, EntryLogError> {
        let storage = DirectoryStorage::open(directory).map_err(|_| GroupLogError::Unavailable)?;
        Self::open_with(Arc::new(storage), limits)
    }

    /// Opens the entry log kept in `storage` and replays its records.
    ///
    /// # Errors
    /// The group log's refusals, a record this log did not write, and an
    /// entry that does not follow the one before it.
    pub fn open_with(
        storage: Arc<dyn SegmentStorage>,
        limits: GroupLogLimits,
    ) -> Result<Self, EntryLogError> {
        let (log, recovered) = GroupLog::open_with(storage, limits)?;
        let mut state = State::default();
        for (position, record) in recovered.records.iter().enumerate() {
            match codec::decode(record).ok_or(EntryLogError::Malformed { record: position })? {
                Record::Entry(id, bytes) => {
                    let segment = segment_of(&recovered.segments, &recovered.starts, position);
                    state.push(id, bytes.into(), segment)?;
                }
                Record::Truncate(after) => state.truncate(after),
                Record::Vote(vote) => state.vote = Some(vote),
                Record::Purge(upto) => state.purge(upto),
            }
        }
        let log = Arc::new(log);
        Ok(Self {
            notifier: Notifier::start(Arc::clone(&log))?,
            state: Mutex::new(state),
            log,
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The last purged id and the last entry's id (the purged one when no
    /// entry is kept).
    #[must_use]
    pub fn bounds(&self) -> (Option<EntryId>, Option<EntryId>) {
        let state = self.state();
        (state.purged, state.last())
    }

    /// The last saved vote.
    #[must_use]
    pub fn vote(&self) -> Option<Vec<u8>> {
        self.state().vote.clone()
    }

    fn write_durably(&self, records: Vec<Vec<u8>>) -> Result<(), EntryLogError> {
        let sequence = self.log.append_batch(records)?;
        self.log.wait_durable(sequence)?;
        Ok(())
    }

    /// Saves `vote`; returns once it is durable.
    ///
    /// # Errors
    /// The group log's refusals.
    pub fn save_vote(&self, vote: &[u8]) -> Result<(), EntryLogError> {
        self.write_durably(vec![codec::vote(vote)])?;
        self.state().vote = Some(vote.to_vec());
        Ok(())
    }

    /// Appends entries that follow the last one; readers see them at once
    /// and `done` is called once they are durable. Entries too large for one
    /// block together go in several batches.
    ///
    /// # Errors
    /// A gap, and the group log's refusals; `done` is then not called.
    pub fn append(
        &self,
        entries: Vec<(EntryId, Vec<u8>)>,
        done: Durable,
    ) -> Result<(), EntryLogError> {
        let mut state = self.state();
        let mut next = state.last().and_then(|last| last.index.checked_add(1));
        for (id, _) in &entries {
            if next.is_some_and(|next| id.index > next) {
                return Err(EntryLogError::Gap { index: id.index });
            }
            next = id.index.checked_add(1);
        }
        let max_bytes = self
            .log
            .limits()
            .max_block_bytes
            .checked_div(2)
            .unwrap_or(0);
        let mut chunk: Vec<(EntryId, Arc<[u8]>)> = Vec::new();
        let mut records: Vec<Vec<u8>> = Vec::new();
        let mut chunk_bytes = 0_usize;
        let mut sequence = None;
        for (id, bytes) in entries {
            let record = codec::entry(id, &bytes);
            let size = record.len().saturating_add(4);
            if !records.is_empty() && chunk_bytes.saturating_add(size) > max_bytes {
                sequence = Some(self.write_chunk(&mut state, &mut chunk, &mut records)?);
                chunk_bytes = 0;
            }
            chunk_bytes = chunk_bytes.saturating_add(size);
            chunk.push((id, bytes.into()));
            records.push(record);
        }
        if !records.is_empty() {
            sequence = Some(self.write_chunk(&mut state, &mut chunk, &mut records)?);
        }
        drop(state);
        match sequence {
            Some(sequence) => self.notifier.notify(sequence, done),
            None => done(Ok(())),
        }
        Ok(())
    }

    /// Queues one batch and keeps its entries once the log accepted it.
    fn write_chunk(
        &self,
        state: &mut State,
        chunk: &mut Vec<(EntryId, Arc<[u8]>)>,
        records: &mut Vec<Vec<u8>>,
    ) -> Result<u64, EntryLogError> {
        let segment = self.log.current_segment();
        let sequence = self.log.append_batch(std::mem::take(records))?;
        for (id, bytes) in chunk.drain(..) {
            state.push(id, bytes, segment)?;
        }
        Ok(sequence)
    }

    /// The kept entries with indexes in `range`, in order.
    #[must_use]
    pub fn entries(&self, range: Range<u64>) -> Vec<(EntryId, Arc<[u8]>)> {
        let state = self.state();
        let Some(first) = state.entries.front().map(|kept| kept.id.index) else {
            return Vec::new();
        };
        let skip = range.start.saturating_sub(first);
        let count = range.end.saturating_sub(range.start.max(first));
        state
            .entries
            .iter()
            .skip(usize::try_from(skip).unwrap_or(usize::MAX))
            .take(usize::try_from(count).unwrap_or(usize::MAX))
            .map(|kept| (kept.id, Arc::clone(&kept.bytes)))
            .collect()
    }

    /// Drops every entry after index `after` (all with `None`); returns once
    /// durable.
    ///
    /// # Errors
    /// The group log's refusals.
    pub fn truncate_after(&self, after: Option<u64>) -> Result<(), EntryLogError> {
        self.write_durably(vec![codec::truncate(after)])?;
        self.state().truncate(after);
        Ok(())
    }

    /// Drops the entries up to `upto` and removes the segments that hold
    /// only those; returns once durable.
    ///
    /// # Errors
    /// The group log's refusals.
    pub fn purge(&self, upto: EntryId) -> Result<(), EntryLogError> {
        let vote = self.vote();
        let mut records = vec![codec::purge(upto)];
        records.extend(vote.as_deref().map(codec::vote));
        // The markers land in this segment or a later one.
        let markers = self.log.current_segment();
        self.write_durably(records)?;
        let keep_from = {
            let mut state = self.state();
            state.purge(upto);
            state
                .entries
                .front()
                .map_or(markers, |kept| kept.segment.min(markers))
        };
        self.log.remove_before(keep_from)?;
        Ok(())
    }
}

fn port_error(error: impl fmt::Display) -> ConsensusLogError {
    ConsensusLogError::new(error.to_string())
}

impl ConsensusLog for EntryLog {
    fn bounds(&self) -> (Option<EntryId>, Option<EntryId>) {
        Self::bounds(self)
    }

    fn vote(&self) -> Option<Vec<u8>> {
        Self::vote(self)
    }

    fn save_vote(&self, vote: &[u8]) -> Result<(), ConsensusLogError> {
        Self::save_vote(self, vote).map_err(port_error)
    }

    fn append(
        &self,
        entries: Vec<(EntryId, Vec<u8>)>,
        done: wf_ports::consensus::log::Durable,
    ) -> Result<(), ConsensusLogError> {
        Self::append(
            self,
            entries,
            Box::new(move |durable| done(durable.map_err(port_error))),
        )
        .map_err(port_error)
    }

    fn entries(&self, range: Range<u64>) -> Vec<(EntryId, Arc<[u8]>)> {
        Self::entries(self, range)
    }

    fn truncate_after(&self, after: Option<u64>) -> Result<(), ConsensusLogError> {
        Self::truncate_after(self, after).map_err(port_error)
    }

    fn purge(&self, upto: EntryId) -> Result<(), ConsensusLogError> {
        Self::purge(self, upto).map_err(port_error)
    }
}
