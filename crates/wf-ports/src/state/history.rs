//! Owned physical history capture; authority and lifecycle visibility stay external.

use wf_domain::history::HistoryEntry;
use wf_domain::workflow::control::ControlState;

use super::StoredWorkflowRecord;
use crate::blob::WorkflowValueBindingKnowledge;

mod hydration;
pub use hydration::{
    WorkflowHistoryHydrationError, WorkflowHistoryHydrationLimits, WorkflowHistoryHydrationRequest,
    WorkflowHistoryValuePage,
};

/// Owned represented-core postimage and event, without read/commit authority.
///
/// Rich source extensions and actual value provenance remain separate companion
/// obligations; this data does not invent their absence or source compatibility.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredWorkflowHistorySnapshot {
    /// Complete represented event, including resolved retry observations.
    pub event: HistoryEntry,
    /// Every represented core field and its exact scoped ordering metadata.
    pub postimage: StoredWorkflowRecord,
}

/// Physical numeric-millisecond/original-ID traversal direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkflowHistoryDirection {
    /// Smallest physical ordering tuple first.
    Ascending,
    /// Largest physical ordering tuple first.
    Descending,
}

/// Explicit physical page selection within one record and coherent snapshot.
#[derive(Clone, Copy, Debug)]
pub struct WorkflowHistoryQuery<'a> {
    /// Positive maximum number of selected rows, excluding lookahead.
    pub limit: usize,
    /// Exclusive original event ID; its physical row need not exist.
    pub boundary_event: Option<&'a str>,
    /// Forward or reverse physical order.
    pub direction: WorkflowHistoryDirection,
    /// Optional exact control precondition; disagreement returns `KnownAborted`.
    pub expected_control: Option<ControlState>,
}

/// Independent logical query ceilings, separate from engine pages and RSS.
#[derive(Clone, Copy, Debug)]
pub struct WorkflowHistoryReadLimits {
    /// Maximum requested selected rows; lookahead is additional.
    pub maximum_entries: usize,
    /// Maximum point/range candidates, including missing/end probes.
    pub maximum_row_reads: usize,
    /// Cumulative admitted physical key/value bytes inspected.
    pub maximum_inspected_bytes: usize,
    /// Complete returned page, bindings, selected rows and owned lookahead.
    pub maximum_returned_bytes: usize,
    /// Continuous logical work ceiling across the complete read.
    pub maximum_work: usize,
    /// Simultaneous named query/guard/working/returned ownership.
    pub maximum_temporary_bytes: usize,
}

/// One complete persisted event and its actual historical value provenance.
pub struct StoredWorkflowHistoryRow {
    /// Original event identity, metadata and retry observations.
    pub event: HistoryEntry,
    /// Actual companion, or explicit legacy unavailability.
    pub values: WorkflowValueBindingKnowledge,
}

/// Owned physical candidates from one coherent read snapshot.
/// Caller applies lifecycle visibility and its authoritative read barrier.
pub struct WorkflowHistoryPage {
    /// Actual group control observed in the same snapshot.
    pub control: ControlState,
    /// Physical current-record version; absence produces an empty capture.
    pub record_version: Option<u64>,
    /// Actual current lifecycle/value provenance, including legacy unavailability.
    pub current_values: WorkflowValueBindingKnowledge,
    /// Selected owned events in the requested physical order.
    pub entries: Vec<StoredWorkflowHistoryRow>,
    /// The next complete candidate, retained for later hydration before trimming.
    pub lookahead: Option<StoredWorkflowHistoryRow>,
    /// Last selected original ID when lookahead proves there are more rows.
    pub continuation: Option<String>,
}

impl WorkflowHistoryPage {
    /// Whether an actual additional candidate was captured.
    #[must_use]
    pub const fn has_more(&self) -> bool {
        self.lookahead.is_some()
    }
}
