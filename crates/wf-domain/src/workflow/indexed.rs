//! Canonical priority/sequence and borrowed correctness-index write descriptions.

use std::collections::BTreeMap;

use wf_types::identity::{GroupId, PartitionId};
use wf_types::version::LogIdentity;

use crate::claim::{
    ClaimRecord, DueOrder, FifoIndex, LaneKey, LaneOrder, ParallelIndex, RecordIndex,
    ScopedRecordId,
};
use crate::policy::InstalledRetryPolicy;
use crate::policy::definition::{PolicySnapshot, StateMode};

use super::admission::AdmissionLimits;
use super::batch::{ApplyError, BatchItem, BatchPlan, BatchRejection, GroupView};

mod authority;
mod decision;
mod independent;
pub(crate) use authority::IndexedAuthority;
pub(crate) use decision::{decide_selected_claims, decide_selected_claims_touched};
pub use independent::{
    IndependentIndexedDecision, IndependentIndexedPlan,
    decide_public_mutation_touched_indexed_batch, decide_touched_independent_indexed_batch,
};

/// Current canonical records, modes and complete correctness indexes.
///
/// The adapter serializes this coherent authority and supplies all relevant lane
/// members. Physical index keys/head/due writes remain its atomic persistence
/// duty. A candidate window or client mode/priority hint cannot supply authority.
#[derive(Clone, Copy, Debug)]
pub struct IndexedGroupView<'a> {
    /// Complete relevant canonical record and group placement values.
    pub batch: GroupView<'a>,
    /// Canonical ordering metadata bound to the same current record values.
    pub records: &'a RecordIndex<'a>,
    /// Complete current parallel due authority.
    pub parallel: &'a ParallelIndex<'a>,
    /// Complete FIFO lanes, including running heads and every follower.
    pub fifo: &'a FifoIndex<'a>,
    /// Coherent installed policies keyed by exact workflow type; absent is default.
    pub policies: &'a BTreeMap<&'a [u8], &'a PolicySnapshot>,
}

/// Explicit observed points, without claiming complete index or record maps.
///
/// A missing reached map key is unobserved and refuses; present `None`/`false`
/// represents actual absence. Query descriptors and policy views may drop after
/// reduction; only canonical current-record borrows can enter the returned plan.
/// These ordinary mathematical facts convey no transaction provenance.
#[derive(Clone, Copy, Debug)]
pub struct TouchedIndexedGroupView<'query, 'current> {
    /// Expected coherent group for authoritative partition placement.
    pub group: &'query GroupId,
    /// Observed partition placement; missing placement remains invalid authority.
    pub placements: &'query BTreeMap<PartitionId, GroupId>,
    /// Observed exact canonical point, including explicit record absence.
    pub current: &'query BTreeMap<ScopedRecordId<'query>, Option<ClaimRecord<'current>>>,
    /// Observed exact parallel due membership, including explicit false.
    pub parallel: &'query BTreeMap<DueOrder<'query>, bool>,
    /// Observed exact FIFO member; a present member proves its lane is nonempty.
    pub fifo_members:
        &'query BTreeMap<(LaneKey<'query>, LaneOrder<'query>), Option<ClaimRecord<'current>>>,
    /// Observed first FIFO member or actual empty prefix, never a projection.
    pub fifo_heads:
        &'query BTreeMap<LaneKey<'query>, Option<(LaneOrder<'current>, ClaimRecord<'current>)>>,
    /// Observed installed mode policy; present `None` supplies source defaults.
    pub policies: &'query BTreeMap<&'query [u8], Option<&'query PolicySnapshot>>,
    /// Observed retry policy, overriding item policy even for present `None`.
    pub retry_policies: &'query BTreeMap<&'query [u8], Option<&'query InstalledRetryPolicy>>,
}

/// One bounded item with creation priority; existing priority remains canonical.
#[derive(Clone, Copy, Debug)]
pub struct IndexedBatchItem<'a> {
    /// Explicit command and actual encoding inputs.
    pub item: BatchItem<'a>,
    /// Source priority 0..2, consumed only for creation.
    pub create_priority: u8,
}
impl<'a> IndexedBatchItem<'a> {
    /// Prepares an item using the source default creation priority zero.
    #[must_use]
    pub const fn new(item: BatchItem<'a>) -> Self {
        Self {
            item,
            create_priority: 0,
        }
    }
}

/// Explicit log identity and next available per-entry sequence ordinal.
///
/// The outer entry dispatcher supplies this cursor after preceding represented
/// subcommands; it must never restart the ordinal for another batch in the entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntryOrder {
    /// Full committed log identity; its scoped index is the source sequence base.
    pub identity: LogIdentity,
    /// First available offset in 0..=2^32-1, with no implicit wrap.
    pub first_ordinal: u32,
}

/// Canonical ordering scalars persisted with the proposed record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrderingMetadata {
    /// Admitted source priority band, immutable on represented existing updates.
    pub priority: u8,
    /// Exact logical-state entry sequence, retained across claim and same-state retry.
    pub state_enter_seq: u128,
}

/// Semantic correctness row, borrowing actual immutable record bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexRows<'a> {
    /// Parallel due key has no FIFO lane member.
    Parallel {
        /// Complete semantic due ordering identity.
        due: DueOrder<'a>,
    },
    /// FIFO member update also identifies the lane whose head/due must be updated.
    Fifo {
        /// Exact affected logical lane.
        lane: LaneKey<'a>,
        /// Running score and exact sequence/ID member ordering.
        member: LaneOrder<'a>,
        /// Optional due candidate; an unscheduled authoritative head blocks its lane.
        head_candidate: Option<DueOrder<'a>>,
    },
}

/// Before/after semantic changes; unchanged/no-op rows produce two absent values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexDelta<'a> {
    /// Previous row to remove, if changed.
    pub before: Option<IndexRows<'a>>,
    /// Proposed row to insert, if changed.
    pub after: Option<IndexRows<'a>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OrderingChange<'a> {
    before: Option<ClaimRecord<'a>>,
    before_mode: StateMode,
    after: OrderingMetadata,
    after_mode: StateMode,
}

/// Complete bounded record/history/ordering description, borrowing predecessors.
///
/// Keep the coherent predecessor owner alive while using/encoding deltas. M2
/// must update affected FIFO heads from its authoritative lane transaction and
/// persist records, metadata, due/lane/head, history and outcome atomically.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedBatchPlan<'a> {
    batch: BatchPlan,
    ordering: Vec<OrderingChange<'a>>,
    order: EntryOrder,
    consumed: u32,
}
impl IndexedBatchPlan<'_> {
    /// Borrows touched-only records/history in original command order.
    #[must_use]
    pub const fn batch(&self) -> &BatchPlan {
        &self.batch
    }
    /// Borrows canonical proposed ordering values in original command order.
    pub fn metadata(&self) -> impl ExactSizeIterator<Item = OrderingMetadata> + '_ {
        self.ordering.iter().map(|change| change.after)
    }
    /// Derives borrowed before/after rows without copying keys or record fields.
    pub fn deltas(&self) -> impl ExactSizeIterator<Item = IndexDelta<'_>> {
        self.batch
            .items()
            .iter()
            .zip(&self.ordering)
            .map(|(item, change)| {
                let before = change
                    .before
                    .and_then(|entry| rows(entry, change.before_mode));
                let after = item.plan().write().and_then(|write| {
                    rows(
                        ClaimRecord {
                            record: &write.record,
                            partition_key: item.partition().as_bytes(),
                            priority: change.after.priority,
                            state_enter_seq: change.after.state_enter_seq,
                        },
                        change.after_mode,
                    )
                });
                if item.plan().write().is_none() || before == after {
                    IndexDelta {
                        before: None,
                        after: None,
                    }
                } else {
                    IndexDelta { before, after }
                }
            })
    }
    /// Counts only successful creation/logical-state allocations.
    #[must_use]
    pub const fn consumed_ordinals(&self) -> u32 {
        self.consumed
    }
    /// Reads the next ordinal; absent means all 2^32 offsets are consumed.
    #[must_use]
    pub const fn next_ordinal(&self) -> Option<u32> {
        self.order.first_ordinal.checked_add(self.consumed)
    }
}

/// Atomic indexed acceptance or deterministic rejection without partial deltas.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IndexedDecision<'a> {
    /// Complete proposed records/history/ordering/index changes.
    Accepted(IndexedBatchPlan<'a>),
    /// No record, history, index or ordinal mutation is exposed.
    Rejected(IndexRejection),
}
/// Additional represented index guards, distinct from corrupt authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexRejection {
    /// Original atomic-record input/domain guard failed.
    Batch(BatchRejection),
    /// Creation priority is outside the source bands zero through two.
    InvalidPriority,
    /// Source FIFO entry permits priority zero only.
    FifoPriority,
    /// Named claim/reclaim is not the real authoritative FIFO head.
    NotFifoHead,
}

/// Decides bounded canonical record/index changes at one explicit committed entry.
///
/// Exact log/request replay must be handled before calling this fresh reducer.
/// This value is not an authorization or durable-apply certificate.
///
/// # Errors
/// Rejects corrupt consumed authority, ordinal overflow or host allocation failure.
pub fn decide_indexed_batch<'view>(
    view: IndexedGroupView<'view>,
    items: &[IndexedBatchItem<'_>],
    order: EntryOrder,
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<IndexedDecision<'view>, ApplyError> {
    decision::decide(
        authority::IndexedAuthority::Full(view),
        items,
        order,
        encoded_entry,
        limits,
    )
}

/// Decides from explicit reached observations without constructing complete views.
///
/// Current point owners and all observations must share one coherent revision.
/// Absent observations refuse; observed policy/retry absence supplies defaults.
/// Query maps and installed policies are not retained by the returned plan.
///
/// # Errors
/// Rejects unobserved/corrupt authority, ordinal overflow or host allocation failure.
pub fn decide_touched_indexed_batch<'current>(
    view: TouchedIndexedGroupView<'_, 'current>,
    items: &[IndexedBatchItem<'_>],
    order: EntryOrder,
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<IndexedDecision<'current>, ApplyError> {
    decision::decide(
        authority::IndexedAuthority::Touched(view),
        items,
        order,
        encoded_entry,
        limits,
    )
}

fn rows(entry: ClaimRecord<'_>, mode: StateMode) -> Option<IndexRows<'_>> {
    match mode {
        StateMode::Parallel => Some(IndexRows::Parallel {
            due: DueOrder::for_record(&entry)?,
        }),
        StateMode::Fifo => Some(IndexRows::Fifo {
            lane: LaneKey::for_record(&entry)?,
            member: LaneOrder::for_record(&entry),
            head_candidate: DueOrder::for_record(&entry),
        }),
    }
}
