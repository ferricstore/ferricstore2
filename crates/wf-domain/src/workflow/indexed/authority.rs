//! Closed complete/observed correctness facts with current-only returned borrows.

use crate::claim::{ClaimRecord, DueOrder, LaneKey, LaneOrder, ScopedRecordId};
use crate::policy::definition::StateMode;
use crate::workflow::WorkflowRecord;
use crate::workflow::batch::{ApplyError, BatchAuthority, BatchItem, observed_current};

use super::{IndexedGroupView, TouchedIndexedGroupView};

#[derive(Clone, Copy)]
pub(super) enum IndexedAuthority<'query, 'current> {
    Full(IndexedGroupView<'current>),
    Touched(TouchedIndexedGroupView<'query, 'current>),
}

impl<'query, 'current> IndexedAuthority<'query, 'current> {
    pub(super) fn batch(self) -> BatchAuthority<'query, 'current> {
        match self {
            Self::Full(view) => BatchAuthority::Full(view.batch),
            Self::Touched(view) => BatchAuthority::Touched(view),
        }
    }

    pub(super) fn current(
        self,
        item: BatchItem<'_>,
    ) -> Result<Option<ClaimRecord<'current>>, ApplyError> {
        let id = ScopedRecordId {
            partition_key: item.partition.as_bytes(),
            id: item.command.id.as_bytes(),
        };
        match self {
            Self::Touched(view) => observed_current(view, id),
            Self::Full(view) => match (view.batch.records.get(&id), view.records.get(&id)) {
                (None, None) => Ok(None),
                (Some(record), Some(entry))
                    if entry.record == *record && ScopedRecordId::for_record(entry) == id =>
                {
                    Ok(Some(*entry))
                }
                _ => Err(ApplyError::InvalidAuthority),
            },
        }
    }

    pub(super) fn canonical(
        self,
        id: ScopedRecordId<'_>,
    ) -> Result<&'current WorkflowRecord, ApplyError> {
        self.batch().record(id)?.ok_or(ApplyError::InvalidAuthority)
    }

    pub(super) fn canonical_matches(
        self,
        id: ScopedRecordId<'_>,
        head: ClaimRecord<'_>,
    ) -> Result<bool, ApplyError> {
        match self {
            Self::Full(view) => Ok(view.records.get(&id) == Some(&head)),
            Self::Touched(view) => Ok(observed_current(view, id)? == Some(head)),
        }
    }

    pub(super) fn mode(self, record: &WorkflowRecord) -> Result<StateMode, ApplyError> {
        let policy = match self {
            Self::Full(view) => view.policies.get(record.workflow_type()).copied(),
            Self::Touched(view) => view
                .policies
                .get(record.workflow_type())
                .copied()
                .ok_or(ApplyError::InvalidAuthority)?,
        };
        let Some(policy) = policy else {
            return Ok(StateMode::Parallel);
        };
        if policy.workflow_type() != record.workflow_type() {
            return Err(ApplyError::InvalidAuthority);
        }
        Ok(policy
            .states()
            .get(logical_state(record))
            .and_then(|state| state.mode)
            .unwrap_or(StateMode::Parallel))
    }

    pub(super) fn parallel_member(
        self,
        due: DueOrder<'_>,
        entry: ClaimRecord<'_>,
    ) -> Result<bool, ApplyError> {
        match self {
            Self::Full(view) => Ok(view.parallel.get(&due) == Some(&entry)),
            Self::Touched(view) => view
                .parallel
                .get(&due)
                .copied()
                .ok_or(ApplyError::InvalidAuthority),
        }
    }

    pub(super) fn parallel_contains(self, due: DueOrder<'_>) -> Result<bool, ApplyError> {
        match self {
            Self::Full(view) => Ok(view.parallel.contains_key(&due)),
            Self::Touched(view) => view
                .parallel
                .get(&due)
                .copied()
                .ok_or(ApplyError::InvalidAuthority),
        }
    }

    pub(super) fn lane_present(self, lane: LaneKey<'_>) -> Result<bool, ApplyError> {
        match self {
            Self::Full(view) => Ok(view.fifo.contains_key(&lane)),
            Self::Touched(view) => view
                .fifo_heads
                .get(&lane)
                .map(Option::is_some)
                .ok_or(ApplyError::InvalidAuthority),
        }
    }

    pub(super) fn member(
        self,
        lane: LaneKey<'_>,
        member: LaneOrder<'_>,
    ) -> Result<Option<ClaimRecord<'current>>, ApplyError> {
        match self {
            Self::Full(view) => Ok(view
                .fifo
                .get(&lane)
                .ok_or(ApplyError::InvalidAuthority)?
                .get(&member)
                .copied()),
            Self::Touched(view) => view
                .fifo_members
                .get(&(lane, member))
                .copied()
                .ok_or(ApplyError::InvalidAuthority),
        }
    }

    pub(super) fn head(
        self,
        lane: LaneKey<'_>,
    ) -> Result<(LaneOrder<'current>, ClaimRecord<'current>), ApplyError> {
        match self {
            Self::Full(view) => view
                .fifo
                .get(&lane)
                .ok_or(ApplyError::InvalidAuthority)?
                .first_key_value()
                .map(|(order, row)| (*order, *row))
                .ok_or(ApplyError::InvalidAuthority),
            Self::Touched(view) => view
                .fifo_heads
                .get(&lane)
                .copied()
                .flatten()
                .ok_or(ApplyError::InvalidAuthority),
        }
    }
}

pub(super) fn logical_state(record: &WorkflowRecord) -> &[u8] {
    if record.state() == b"running" {
        record.run_state().unwrap_or(b"queued")
    } else {
        record.state()
    }
}

pub(super) fn active(record: &WorkflowRecord) -> bool {
    !matches!(
        logical_state(record),
        b"completed" | b"failed" | b"cancelled"
    )
}
