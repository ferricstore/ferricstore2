//! Bounded borrowed results with explicit partial-work accounting.

use crate::workflow::WorkflowRecord;

use super::{DueOrder, SelectionRequest};

/// The operation a future atomic apply adapter must revalidate and persist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionAction {
    /// Claim a due unleased record.
    Claim,
    /// Reclaim this exact expired running record, preserving its logical state.
    Reclaim,
}

/// Borrowed selected identity/revision; this value is not a durability acknowledgement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectedClaim<'a> {
    /// Current record, including predecessor revision and fencing identity.
    pub record: &'a WorkflowRecord,
    /// Authoritative partition of the selected record.
    pub partition_key: &'a [u8],
    /// Explicit claim or expired-reclaim action.
    pub action: SelectionAction,
}

/// Why bounded selection stopped, independently of selected cardinality.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionStop {
    /// Every represented relevant index row/head has been inspected for this snapshot.
    Complete,
    /// Output limit reached while more rows remain; continuation is available.
    ClaimLimit,
    /// Candidate scan budget reached while more rows remain; continuation is available.
    ScanBudget,
}

/// A borrowed private cursor for the same immutable snapshot and exact request.
///
/// It is not a durable client cursor. Mutations invalidate the snapshot and require
/// fresh authoritative selection; reusing this cursor cannot authorize a write.
#[derive(Clone, Copy, Debug)]
pub struct SelectionContinuation<'a> {
    pub(super) group: &'a [u8],
    pub(super) revision: u64,
    pub(super) request: SelectionRequest<'a>,
    pub(super) parallel_after: Option<DueOrder<'a>>,
    pub(super) fifo_after: Option<DueOrder<'a>>,
}

/// A bounded selection result that keeps work exhaustion distinct from empty work.
#[derive(Clone, Debug)]
pub struct ClaimSelection<'a> {
    pub(super) records: Vec<SelectedClaim<'a>>,
    pub(super) scanned: usize,
    pub(super) head_reads: usize,
    pub(super) stop: SelectionStop,
    pub(super) continuation: Option<SelectionContinuation<'a>>,
}

impl<'a> ClaimSelection<'a> {
    /// Borrows at most the requested number of selected records, in total due order.
    #[must_use]
    pub fn records(&self) -> &[SelectedClaim<'a>] {
        &self.records
    }

    /// Returns the number of merged rows inspected under the candidate budget.
    #[must_use]
    pub const fn scanned(&self) -> usize {
        self.scanned
    }

    /// Returns separately bounded direct lane-head lookups, without follower scans.
    #[must_use]
    pub const fn head_reads(&self) -> usize {
        self.head_reads
    }

    /// Reports completion or the exact limit that stopped selection.
    #[must_use]
    pub const fn stop(&self) -> SelectionStop {
        self.stop
    }

    /// Borrows a same-snapshot continuation only when uninspected rows remain.
    #[must_use]
    pub const fn continuation(&self) -> Option<&SelectionContinuation<'a>> {
        self.continuation.as_ref()
    }
}
