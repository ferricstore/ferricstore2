//! Bounded record/history write descriptions; no persistence is performed.

use wf_types::version::TimestampMillis;

use crate::history::{EventKind, HistoryEntry, LatestEvent, RetryHistory};

use super::WorkflowRecord;

/// Semantic result of a successful decision, distinct from durable application.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// A validated record mutation and history event are described.
    Changed,
    /// A source-permitted repeated effect has no new writes or event.
    LegacyNoop,
    /// This explicit record is not eligible for claim/reclaim at committed time.
    NotEligible,
}

/// One affected record and the predecessor version required for atomic apply.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordWrite {
    /// Absent for create; otherwise the authoritative predecessor version.
    pub expected_version: Option<u64>,
    /// The proposed new immutable record.
    pub record: WorkflowRecord,
}

/// A bounded write description; this type does not persist or apply itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionPlan {
    outcome: Outcome,
    write: Option<RecordWrite>,
    history: Option<HistoryEntry>,
}

impl TransitionPlan {
    /// Reads whether a change, legacy duplicate or skipped claim was decided.
    #[must_use]
    pub const fn outcome(&self) -> Outcome {
        self.outcome
    }
    /// Borrows the only affected-record write, if any.
    #[must_use]
    pub const fn write(&self) -> Option<&RecordWrite> {
        self.write.as_ref()
    }
    /// Borrows the only new history entry, if any.
    #[must_use]
    pub const fn history(&self) -> Option<&HistoryEntry> {
        self.history.as_ref()
    }
    /// Transfers the bounded write description to an atomic-apply adapter.
    #[must_use]
    pub fn into_parts(self) -> (Outcome, Option<RecordWrite>, Option<HistoryEntry>) {
        (self.outcome, self.write, self.history)
    }
}

pub(super) fn noop_plan(outcome: Outcome) -> TransitionPlan {
    TransitionPlan {
        outcome,
        write: None,
        history: None,
    }
}

pub(super) fn changed_plan(
    previous: Option<&WorkflowRecord>,
    record: WorkflowRecord,
    kind: EventKind,
    now: TimestampMillis,
) -> TransitionPlan {
    build_plan(previous, record, kind, now, None)
}

pub(super) fn changed_retry_plan(
    previous: &WorkflowRecord,
    record: WorkflowRecord,
    now: TimestampMillis,
    retry: RetryHistory,
) -> TransitionPlan {
    build_plan(Some(previous), record, EventKind::Retry, now, Some(retry))
}

fn build_plan(
    previous: Option<&WorkflowRecord>,
    mut record: WorkflowRecord,
    kind: EventKind,
    now: TimestampMillis,
    retry: Option<RetryHistory>,
) -> TransitionPlan {
    record.latest_event = LatestEvent {
        kind,
        version: record.version,
        fence: record.fence,
    };
    let event_time = previous.map_or(now, |previous| now.max(previous.updated_at));
    let history = HistoryEntry {
        event_id: format!("{}-{}", event_time.get(), record.version),
        kind,
        occurred_at: now,
        version: record.version,
        fence: record.fence,
        attempts: record.attempts,
        state: record.state.clone(),
        next_run_at: record.next_run_at,
        lease_deadline: record.lease_deadline(),
        worker: record.lease.as_ref().map(|lease| lease.worker.clone()),
        retry,
    };
    TransitionPlan {
        outcome: Outcome::Changed,
        write: Some(RecordWrite {
            expected_version: previous.map(WorkflowRecord::version),
            record,
        }),
        history: Some(history),
    }
}
