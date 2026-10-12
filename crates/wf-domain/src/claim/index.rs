//! Borrowed correctness indexes and exact FIFO member order.

use std::cmp::{Ordering, Reverse};
use std::collections::BTreeMap;

use wf_types::identity::RecordId;
use wf_types::version::TimestampMillis;

use crate::workflow::{WorkflowRecord, WorkflowStatus};

/// A borrowed authoritative record with ordering metadata absent from the core record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClaimRecord<'a> {
    /// Current immutable authoritative workflow record.
    pub record: &'a WorkflowRecord,
    /// Authoritative storage partition; never a client routing hint.
    pub partition_key: &'a [u8],
    /// Source priority band, validated as 0 through 2 during selection.
    pub priority: u8,
    /// Exact committed logical-state entry sequence, preserved across claim/retry.
    pub state_enter_seq: u128,
}

impl<'a> ClaimRecord<'a> {
    /// Returns the logical state retained separately during running execution.
    #[must_use]
    pub fn logical_state(self) -> &'a [u8] {
        if self.record.status() == WorkflowStatus::Running {
            self.record.run_state().unwrap_or(b"queued")
        } else {
            self.record.state()
        }
    }
}

/// A total due index key: higher priority first, then due time and binary identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DueOrder<'a> {
    /// Source priority band.
    pub priority: u8,
    /// Authoritative scheduled time or lease deadline.
    pub due: TimestampMillis,
    /// Exact binary record ID.
    pub id: &'a [u8],
    /// Storage partition tie-break, after the source due/id comparison.
    pub partition_key: &'a [u8],
    /// Logical state tie-break, independent of caller filter order.
    pub state: &'a [u8],
    /// Workflow type tie-break.
    pub workflow_type: &'a [u8],
}

impl<'a> DueOrder<'a> {
    /// Constructs the expected authoritative key; terminal/unscheduled records omit it.
    #[must_use]
    pub fn for_record(entry: &ClaimRecord<'a>) -> Option<Self> {
        let lane = LaneKey::for_record(entry)?;
        Some(Self {
            priority: entry.priority,
            due: entry.record.next_run_at()?,
            id: entry.record.id().as_bytes(),
            partition_key: lane.partition_key,
            state: lane.state,
            workflow_type: lane.workflow_type,
        })
    }
}

impl Ord for DueOrder<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        Reverse(self.priority)
            .cmp(&Reverse(other.priority))
            .then_with(|| self.due.get().cmp(&other.due.get()))
            .then_with(|| self.id.cmp(other.id))
            .then_with(|| self.partition_key.cmp(other.partition_key))
            .then_with(|| self.state.cmp(other.state))
            .then_with(|| self.workflow_type.cmp(other.workflow_type))
    }
}

impl PartialOrd for DueOrder<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Semantic FIFO lane identity, without selecting a physical storage key format.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LaneKey<'a> {
    /// Workflow type.
    pub workflow_type: &'a [u8],
    /// Logical run state; running members remain in this lane.
    pub state: &'a [u8],
    /// Explicit partition scope.
    pub partition_key: &'a [u8],
}

impl<'a> LaneKey<'a> {
    /// Derives a source lane identity; logical terminal states have no active entry.
    #[must_use]
    pub fn for_record(entry: &ClaimRecord<'a>) -> Option<Self> {
        let state = entry.logical_state();
        if matches!(state, b"completed" | b"failed" | b"cancelled") {
            None
        } else {
            Some(Self {
                workflow_type: entry.record.workflow_type(),
                state,
                partition_key: entry.partition_key,
            })
        }
    }
}

/// Running members rank before queued members, then exact sequence and binary ID.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LaneOrder<'a> {
    score: i8,
    sequence: u128,
    id: &'a [u8],
}

impl<'a> LaneOrder<'a> {
    /// Derives the source score/order without converting the sequence to a float.
    #[must_use]
    pub fn for_record(entry: &ClaimRecord<'a>) -> Self {
        Self {
            score: if entry.record.status() == WorkflowStatus::Running {
                -1
            } else {
                0
            },
            sequence: entry.state_enter_seq,
            id: entry.record.id().as_bytes(),
        }
    }

    /// Borrows the actual member-key identity for pre-comparison admission.
    #[must_use]
    pub(crate) fn id(self) -> &'a [u8] {
        self.id
    }

    /// Returns the source running (-1) or queued (0) lane score.
    #[must_use]
    pub const fn score(self) -> i8 {
        self.score
    }
}

/// Correctness-critical ordered due index for parallel states.
pub type ParallelIndex<'a> = BTreeMap<DueOrder<'a>, ClaimRecord<'a>>;

/// Complete correctness-critical lane indexes, including running members and followers.
pub type FifoIndex<'a> = BTreeMap<LaneKey<'a>, BTreeMap<LaneOrder<'a>, ClaimRecord<'a>>>;

/// Unique storage identity: workflow type/state do not create a second record identity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ScopedRecordId<'a> {
    /// Authoritative storage partition.
    pub partition_key: &'a [u8],
    /// Exact binary record ID within that partition.
    pub id: &'a [u8],
}

impl<'a> ScopedRecordId<'a> {
    /// Derives the identity used for a canonical current-record lookup.
    #[must_use]
    pub fn for_record(entry: &ClaimRecord<'a>) -> Self {
        Self {
            partition_key: entry.partition_key,
            id: entry.record.id().as_bytes(),
        }
    }
}

/// Borrowed unique current records; ordering rows are checked against this authority.
pub type RecordIndex<'a> = BTreeMap<ScopedRecordId<'a>, ClaimRecord<'a>>;

/// One coherent authoritative apply view; selection borrows indexes without rebuilding them.
///
/// The adapter must provide complete relevant indexes, correct policy mode placement,
/// group placement and a stable revision, and serialize decision with atomic persistence.
/// This value alone cannot prove those obligations or durable success. Parallel rows
/// and FIFO lanes must be disjoint; candidate windows/projections are not valid inputs.
#[derive(Clone, Copy, Debug)]
pub struct AuthoritativeClaimView<'a> {
    /// Authoritative group placement for every represented record/index.
    pub group: &'a [u8],
    /// Value identity of complete coherent contents within this group.
    ///
    /// Same group/revision must mean identical contents, including across reconstructed
    /// equivalent maps. Mutations require a new revision; restoration cannot reuse
    /// one for different contents. Recovery namespacing is an adapter obligation.
    pub revision: u64,
    /// Unique authoritative current records for every represented index row/head.
    pub records: &'a RecordIndex<'a>,
    /// Complete parallel due index for the scope served by this view.
    pub parallel: &'a ParallelIndex<'a>,
    /// Complete lane maps whose first member is available without follower scans.
    pub fifo: &'a FifoIndex<'a>,
}

/// Encodes the pinned exact u128 big-endian sequence followed by binary record ID.
#[must_use]
pub fn encode_member(sequence: u128, id: &RecordId) -> Vec<u8> {
    let mut member = Vec::with_capacity(16 + id.as_bytes().len());
    member.extend_from_slice(&sequence.to_be_bytes());
    member.extend_from_slice(id.as_bytes());
    member
}

/// Decodes a member by borrowing its nonempty ID; truncated/empty-ID members reject.
#[must_use]
pub fn decode_member(member: &[u8]) -> Option<(u128, &[u8])> {
    let (sequence, id) = member.split_at_checked(16)?;
    if id.is_empty() {
        None
    } else {
        Some((u128::from_be_bytes(sequence.try_into().ok()?), id))
    }
}
