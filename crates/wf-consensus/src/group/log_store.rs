//! The consensus log port as `OpenRaft`'s log store: appends report durability through
//! `OpenRaft`'s flush callback from the entry log's notifier thread; votes,
//! truncations and purges return once durable.

use std::fmt::Debug;
use std::future::{Future, ready};
use std::io;
use std::ops::{Bound, Range, RangeBounds};
use std::sync::Arc;

use openraft::OptionalSend;
use openraft::storage::{IOFlushed, LogState, RaftLogReader, RaftLogStorage};
use openraft::type_config::alias::{EntryOf, LogIdOf, VoteOf};
use wf_ports::consensus::log::{ConsensusLog, ConsensusLogError};

use super::Config;
use super::codec;

fn failed(error: ConsensusLogError) -> io::Error {
    io::Error::other(error)
}

/// A group's log store, over the consensus log port; clones share the log.
#[derive(Clone)]
pub struct LogStore {
    log: Arc<dyn ConsensusLog>,
}

impl LogStore {
    /// The log store over `log`.
    #[must_use]
    pub fn new(log: Arc<dyn ConsensusLog>) -> Self {
        Self { log }
    }
}

fn indexes(range: &impl RangeBounds<u64>) -> Range<u64> {
    let start = match range.start_bound() {
        Bound::Included(start) => *start,
        Bound::Excluded(start) => start.saturating_add(1),
        Bound::Unbounded => 0,
    };
    let end = match range.end_bound() {
        Bound::Included(end) => end.saturating_add(1),
        Bound::Excluded(end) => *end,
        Bound::Unbounded => u64::MAX,
    };
    start..end
}

impl LogStore {
    fn append_now(
        &self,
        entries: impl IntoIterator<Item = EntryOf<Config>>,
        callback: IOFlushed<Config>,
    ) -> Result<(), io::Error> {
        let encoded = entries
            .into_iter()
            .map(|entry| Ok((codec::entry_id(&entry.log_id), codec::encode(&entry)?)))
            .collect::<io::Result<Vec<_>>>()?;
        self.log
            .append(
                encoded,
                Box::new(move |durable| callback.io_completed(durable.map_err(io::Error::other))),
            )
            .map_err(failed)
    }
}

// The port answers synchronously, so each method returns a ready future.
impl RaftLogReader<Config> for LogStore {
    fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + OptionalSend>(
        &mut self,
        range: RB,
    ) -> impl Future<Output = Result<Vec<EntryOf<Config>>, io::Error>> + Send {
        ready(
            self.log
                .entries(indexes(&range))
                .into_iter()
                .map(|(id, bytes)| codec::decode(id, &bytes))
                .collect(),
        )
    }

    fn read_vote(
        &mut self,
    ) -> impl Future<Output = Result<Option<VoteOf<Config>>, io::Error>> + Send {
        ready(self.log.vote().as_deref().map(codec::vote).transpose())
    }
}

impl RaftLogStorage<Config> for LogStore {
    type LogReader = Self;

    fn get_log_state(
        &mut self,
    ) -> impl Future<Output = Result<LogState<Config>, io::Error>> + Send {
        let (purged, last) = self.log.bounds();
        ready(Ok(LogState {
            last_purged_log_id: purged.map(codec::log_id),
            last_log_id: last.map(codec::log_id),
        }))
    }

    fn get_log_reader(&mut self) -> impl Future<Output = Self::LogReader> + Send {
        ready(self.clone())
    }

    fn save_vote(
        &mut self,
        vote: &VoteOf<Config>,
    ) -> impl Future<Output = Result<(), io::Error>> + Send {
        ready(self.log.save_vote(&codec::vote_bytes(vote)).map_err(failed))
    }

    fn append<I>(
        &mut self,
        entries: I,
        callback: IOFlushed<Config>,
    ) -> impl Future<Output = Result<(), io::Error>> + Send
    where
        I: IntoIterator<Item = EntryOf<Config>> + OptionalSend,
    {
        ready(self.append_now(entries, callback))
    }

    fn truncate_after(
        &mut self,
        last: Option<LogIdOf<Config>>,
    ) -> impl Future<Output = Result<(), io::Error>> + Send {
        ready(
            self.log
                .truncate_after(last.map(|log_id| log_id.index))
                .map_err(failed),
        )
    }

    fn purge(
        &mut self,
        log_id: LogIdOf<Config>,
    ) -> impl Future<Output = Result<(), io::Error>> + Send {
        ready(self.log.purge(codec::entry_id(&log_id)).map_err(failed))
    }
}
