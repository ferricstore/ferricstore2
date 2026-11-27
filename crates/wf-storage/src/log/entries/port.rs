//! The entry log as the consensus log port: each call is the entry log's
//! own, its refusals as the port's error.

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use wf_ports::consensus::log::{ConsensusLog, ConsensusLogError};

use super::{EntryId, EntryLog};

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

    fn voter(&self) -> bool {
        Self::voter(self)
    }

    fn save_voter(&self) -> Result<(), ConsensusLogError> {
        Self::save_voter(self).map_err(port_error)
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

    fn close(&self) {
        Self::close(self);
    }
}
