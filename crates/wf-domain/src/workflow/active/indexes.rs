//! Borrowed physical semantic rows and correctness-lane descriptions.

use super::ActiveRecord;
use crate::claim::{ClaimRecord, DueOrder, LaneKey, LaneOrder, ScopedRecordId};
use crate::policy::definition::StateMode;
use crate::workflow::batch::ApplyError;

mod heads;
pub(super) use heads::plan_fifo_heads;

/// Semantic source dimensions under the enabled `DueAny` profile.
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub enum ActiveIndexKey<'a> {
    /// Captured nonterminal deadline.
    ActiveTimeout(ScopedRecordId<'a>),
    /// Actual terminal-retention deadline.
    TerminalRetention(ScopedRecordId<'a>),
    /// Physical state due query row, including FIFO followers.
    Due {
        /// Actual workflow type.
        workflow_type: &'a [u8],
        /// Actual physical state.
        state: &'a [u8],
        /// Captured priority.
        priority: u8,
        /// Scoped record.
        record: ScopedRecordId<'a>,
    },
    /// Physical due-any query row, including FIFO followers.
    DueAny {
        /// Actual type.
        workflow_type: &'a [u8],
        /// Captured priority.
        priority: u8,
        /// Scoped record.
        record: ScopedRecordId<'a>,
    },
    /// Physical state membership and updated-time score.
    State {
        /// Actual type.
        workflow_type: &'a [u8],
        /// Physical state.
        state: &'a [u8],
        /// Scoped record.
        record: ScopedRecordId<'a>,
    },
    /// Owned physical running inflight membership.
    Inflight {
        /// Actual type.
        workflow_type: &'a [u8],
        /// Scoped record.
        record: ScopedRecordId<'a>,
    },
    /// Actual nonempty lease owner; nullable encoded keys are unsupported.
    Worker {
        /// Actual worker bytes.
        worker: &'a [u8],
        /// Scoped record.
        record: ScopedRecordId<'a>,
    },
    /// Nonblank captured parent metadata.
    Parent {
        /// Actual value.
        value: &'a [u8],
        /// Scoped record.
        record: ScopedRecordId<'a>,
    },
    /// Nonblank captured root metadata.
    Root {
        /// Actual value.
        value: &'a [u8],
        /// Scoped record.
        record: ScopedRecordId<'a>,
    },
    /// Nonblank captured correlation metadata.
    Correlation {
        /// Actual value.
        value: &'a [u8],
        /// Scoped record.
        record: ScopedRecordId<'a>,
    },
}
/// Fixed borrowed rows; no getter-owned variable key spine is allocated.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ActiveIndexRows<'a> {
    /// Correctness parallel due row, absent in FIFO mode.
    pub parallel: Option<DueOrder<'a>>,
    /// Nonterminal FIFO membership with an independently optional head due row.
    pub fifo: Option<(LaneKey<'a>, LaneOrder<'a>, Option<DueOrder<'a>>)>,
    /// Applicable physical semantic dimensions, at most ten.
    pub semantic: [Option<(ActiveIndexKey<'a>, u64)>; 10],
}
/// Before authority and after final owner rows under one short plan borrow.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ActiveIndexDelta<'a> {
    /// Original rows to delete/update.
    pub before: Option<ActiveIndexRows<'a>>,
    /// Final rows to insert/update.
    pub after: Option<ActiveIndexRows<'a>>,
}
pub(super) type PlannedFifoHead<'v> = (LaneKey<'v>, Option<(Option<usize>, &'v ActiveRecord)>);

pub(super) struct ActiveIndexChange<'a> {
    pub(super) before: &'a ActiveRecord,
    pub(super) write: usize,
    pub(super) before_mode: StateMode,
    pub(super) after_mode: StateMode,
}

/// Describes all rows from one immutable owner; it performs no lookup or equality.
///
/// # Errors
/// Refuses physical running without a supported nonempty actual worker/lease.
pub fn active_rows(
    record: &ActiveRecord,
    mode: StateMode,
) -> Result<ActiveIndexRows<'_>, ApplyError> {
    let record_core = record.core();
    let workflow_type = record_core.workflow_type();
    let state = record_core.state();
    let id = ScopedRecordId {
        partition_key: record.partition().as_bytes(),
        id: record_core.id().as_bytes(),
    };
    let entry = ClaimRecord {
        record: record_core,
        partition_key: id.partition_key,
        priority: record.ordering().priority,
        state_enter_seq: record.ordering().state_enter_seq,
    };
    let terminal = matches!(state, b"completed" | b"failed" | b"cancelled");
    let lane = LaneKey::for_record(&entry);
    let due = DueOrder::for_record(&entry);
    let mut rows = ActiveIndexRows {
        parallel: (mode == StateMode::Parallel).then_some(due).flatten(),
        fifo: (mode == StateMode::Fifo)
            .then_some(lane)
            .flatten()
            .map(|lane| (lane, LaneOrder::for_record(&entry), due)),
        semantic: [None; 10],
    };
    if mode == StateMode::Fifo && !terminal && entry.priority != 0 {
        return Err(ApplyError::InvalidAuthority);
    }
    rows.semantic[0] = if terminal {
        record
            .terminal_retention_until()
            .map(|t| (ActiveIndexKey::TerminalRetention(id), t.get()))
    } else if let Some(duration) = record.captured().max_active_ms() {
        let deadline = record_core
            .created_at()
            .checked_add(duration)
            .ok_or(ApplyError::InvalidAuthority)?;
        Some((ActiveIndexKey::ActiveTimeout(id), deadline.get()))
    } else {
        None
    };
    if let Some(time) = record_core.next_run_at().filter(|_| !terminal) {
        let due = ActiveIndexKey::Due {
            workflow_type,
            state,
            priority: entry.priority,
            record: id,
        };
        let due_any = ActiveIndexKey::DueAny {
            workflow_type,
            priority: entry.priority,
            record: id,
        };
        rows.semantic[1] = Some((due, time.get()));
        rows.semantic[2] = Some((due_any, time.get()));
    }
    let state_key = ActiveIndexKey::State {
        workflow_type,
        state,
        record: id,
    };
    rows.semantic[3] = Some((state_key, record_core.updated_at().get()));
    if state == b"running" {
        let lease = record_core
            .lease()
            .filter(|lease| !lease.worker.is_empty())
            .ok_or(ApplyError::InvalidAuthority)?;
        let inflight = ActiveIndexKey::Inflight {
            workflow_type,
            record: id,
        };
        let worker = ActiveIndexKey::Worker {
            worker: &lease.worker,
            record: id,
        };
        rows.semantic[4] = Some((inflight, lease.deadline.get()));
        rows.semantic[5] = Some((worker, lease.deadline.get()));
    }
    for (slot, name) in [
        record.parent().map(|p| p.id.as_slice()),
        record.root_id(),
        record.correlation_id(),
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(value) = name.filter(|value| !value.is_empty()) {
            let key = match slot {
                0 => ActiveIndexKey::Parent { value, record: id },
                1 => ActiveIndexKey::Root { value, record: id },
                _ => ActiveIndexKey::Correlation { value, record: id },
            };
            rows.semantic[6 + slot] = Some((key, record_core.updated_at().get()));
        }
    }
    Ok(rows)
}

/// Forms explicit before/after descriptions without retaining a substitutable view.
///
/// # Errors
/// Rejects an unsupported row shape; accepted plans validate both sides beforehand.
pub fn index_deltas<'a>(
    before: &'a ActiveRecord,
    after: &'a ActiveRecord,
    before_mode: StateMode,
    after_mode: StateMode,
) -> Result<ActiveIndexDelta<'a>, ApplyError> {
    Ok(ActiveIndexDelta {
        before: Some(active_rows(before, before_mode)?),
        after: Some(active_rows(after, after_mode)?),
    })
}
