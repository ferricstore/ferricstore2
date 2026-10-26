//! Ordered indexed mutations with original-only borrows and every owned history.

use crate::claim::ClaimRecord;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::{
    ApplyError, IndependentProfile, IndependentResults, ScopedPlan, independent_preflight,
    run_independent,
};

use super::authority::IndexedAuthority;
use super::{
    EntryOrder, IndexDelta, IndexRejection, IndexedBatchItem, OrderingChange, OrderingMetadata,
    TouchedIndexedGroupView, rows,
};
use crate::policy::definition::StateMode;

mod step;

struct IndependentIndexState<'query, 'current> {
    authority: IndexedAuthority<'query, 'current>,
    order: EntryOrder,
    limits: AdmissionLimits,
    count: usize,
    ordering: Vec<Option<(OrderingMetadata, StateMode)>>,
    consumed: u32,
    profile: IndependentProfile,
}

/// Owned independent results and original-to-final publication descriptors.
///
/// Originals must remain alive while deltas are used. All intermediate successful
/// histories remain owned here; final descriptors do not borrow those results.
/// Physical final records/indexes and every history still require atomic apply.
#[derive(Debug, Eq, PartialEq)]
pub struct IndependentIndexedPlan<'current> {
    results: Vec<Result<ScopedPlan, IndexRejection>>,
    finals: Vec<(usize, OrderingChange<'current>)>,
    order: EntryOrder,
    consumed: u32,
}

impl<'current> IndependentIndexedPlan<'current> {
    /// Borrows every result in original input order, including errors and no-ops.
    #[must_use]
    pub fn results(&self) -> &[Result<ScopedPlan, IndexRejection>] {
        &self.results
    }

    /// Moves owned results and drops the descriptors borrowing original records.
    #[must_use]
    pub fn into_results(self) -> Vec<Result<ScopedPlan, IndexRejection>> {
        self.results
    }

    /// Selects the last real write per scoped identity, sorted by scoped key.
    pub fn final_indices(&self) -> impl ExactSizeIterator<Item = usize> + '_ {
        self.finals.iter().map(|(index, _)| *index)
    }

    /// Borrows original predecessors, aligned with the sorted final indices.
    pub fn originals(&self) -> impl ExactSizeIterator<Item = Option<ClaimRecord<'current>>> + '_ {
        self.finals.iter().map(|(_, change)| change.before)
    }

    /// Borrows final scalars, aligned with the sorted final indices.
    pub fn metadata(&self) -> impl ExactSizeIterator<Item = OrderingMetadata> + '_ {
        self.finals.iter().map(|(_, change)| change.after)
    }

    /// Derives original-to-final rows while borrowing final records from this owner.
    /// Each invariant failure is operational rather than an omitted publication.
    pub fn deltas(&self) -> impl ExactSizeIterator<Item = Result<IndexDelta<'_>, ApplyError>> + '_ {
        self.finals
            .iter()
            .map(|(index, change)| self.delta(*index, *change))
    }

    /// Visits every real-write history in input order, including intermediate writes.
    pub fn history_plans(&self) -> impl Iterator<Item = &ScopedPlan> + '_ {
        self.results
            .iter()
            .filter_map(|result| result.as_ref().ok())
            .filter(|plan| plan.plan().history().is_some())
    }

    /// Counts successful active-state entries even when later cancelled.
    #[must_use]
    pub const fn consumed_ordinals(&self) -> u32 {
        self.consumed
    }

    /// Reads the next unused ordinal without wrapping the committed position.
    #[must_use]
    pub const fn next_ordinal(&self) -> Option<u32> {
        self.order.first_ordinal.checked_add(self.consumed)
    }

    fn delta(
        &self,
        index: usize,
        change: OrderingChange<'current>,
    ) -> Result<IndexDelta<'_>, ApplyError> {
        let plan = self
            .results
            .get(index)
            .and_then(|result| result.as_ref().ok())
            .ok_or(ApplyError::InvalidAuthority)?;
        let write = plan.plan().write().ok_or(ApplyError::InvalidAuthority)?;
        let before = change
            .before
            .and_then(|entry| rows(entry, change.before_mode));
        let after = rows(
            ClaimRecord {
                record: &write.record,
                partition_key: plan.partition().as_bytes(),
                priority: change.after.priority,
                state_enter_seq: change.after.state_enter_seq,
            },
            change.after_mode,
        );
        Ok(if before == after {
            IndexDelta {
                before: None,
                after: None,
            }
        } else {
            IndexDelta { before, after }
        })
    }
}

/// Ordered item outcomes or a whole-profile refusal exposing no provisional plan.
#[derive(Debug, Eq, PartialEq)]
pub enum IndependentIndexedDecision<'current> {
    /// Every input has an ordered outcome and every real history is retained.
    Accepted(IndependentIndexedPlan<'current>),
    /// Admission/profile failure discarded every provisional write and history.
    Rejected(IndexRejection),
}

/// Independent outcomes and actual ordering for each original input slot.
/// Accepted errors and no-ops have no ordering; rejected profiles return no slots.
pub type IndependentIndexedWithOrdering<'current> = (
    IndependentIndexedDecision<'current>,
    Vec<Option<(OrderingMetadata, StateMode)>>,
);

/// Decides eight independent mutation operations against coherent touched facts.
///
/// Original index authority is checked before accepting its first real write.
/// Later same-ID steps consume owned earlier writes and their scalar ordering.
/// Query maps and commands may drop after return; only original current owners
/// remain borrowed. Admission is cumulative `4V + 2688n`, without refunds.
/// Claim/reclaim and durable publication remain outside this entry point.
///
/// # Errors
/// Refuses invalid authority, ordinal overflow or a fallible output allocation.
pub fn decide_touched_independent_indexed_batch<'current>(
    view: TouchedIndexedGroupView<'_, 'current>,
    items: &[IndexedBatchItem<'_>],
    order: EntryOrder,
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<IndependentIndexedDecision<'current>, ApplyError> {
    decide_profile(
        view,
        items,
        order,
        encoded_entry,
        limits,
        IndependentProfile::Indexed,
    )
    .map(|(decision, _)| decision)
}

/// Decides independent mutations and retains every real write's actual ordering.
///
/// Accepted ordering aligns with original input slots, including errors and no-ops.
/// Whole-profile rejection returns an empty vector. This moves existing kernel
/// storage; callers must admit its retained header and slots in later live phases.
/// Only original current owners remain borrowed by the decision.
///
/// # Errors
/// Refuses invalid authority, ordinal overflow or a fallible output allocation.
pub fn decide_touched_independent_indexed_batch_with_ordering<'current>(
    view: TouchedIndexedGroupView<'_, 'current>,
    items: &[IndexedBatchItem<'_>],
    order: EntryOrder,
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<IndependentIndexedWithOrdering<'current>, ApplyError> {
    decide_profile(
        view,
        items,
        order,
        encoded_entry,
        limits,
        IndependentProfile::Indexed,
    )
}

/// Decides exactly one verified public mutation with source-derived retry timing.
///
/// Only the seven public core operations are admitted. Actual prepared evidence
/// and caller identity must be verified before this Fresh entry; this value is
/// not authorization or publication authority. Old explicit-sample APIs differ.
/// # Errors
/// Refuses count other than one, invalid authority, allocation or ordinal overflow.
pub fn decide_public_mutation_touched_indexed_batch<'current>(
    view: TouchedIndexedGroupView<'_, 'current>,
    items: &[IndexedBatchItem<'_>],
    order: EntryOrder,
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<IndependentIndexedDecision<'current>, ApplyError> {
    if items.len() != 1 {
        return Err(ApplyError::InvalidAuthority);
    }
    decide_profile(
        view,
        items,
        order,
        encoded_entry,
        limits,
        IndependentProfile::Public,
    )
    .map(|(decision, _)| decision)
}

fn decide_profile<'current>(
    view: TouchedIndexedGroupView<'_, 'current>,
    items: &[IndexedBatchItem<'_>],
    order: EntryOrder,
    encoded_entry: &[u8],
    limits: AdmissionLimits,
    profile: IndependentProfile,
) -> Result<IndependentIndexedWithOrdering<'current>, ApplyError> {
    let authority = IndexedAuthority::Touched(view);
    if let Some(error) = independent_preflight(
        authority.batch(),
        items.iter().map(|item| item.item),
        encoded_entry,
        limits,
        profile,
    )? {
        return Ok((
            IndependentIndexedDecision::Rejected(IndexRejection::Batch(error)),
            Vec::new(),
        ));
    }
    let mut commands = Vec::new();
    commands
        .try_reserve_exact(items.len())
        .map_err(|_| ApplyError::AllocationFailed)?;
    commands.extend(items.iter().map(|item| item.item));
    let mut state = IndependentIndexState {
        authority,
        order,
        limits,
        count: items.len(),
        ordering: Vec::new(),
        consumed: 0,
        profile,
    };
    state
        .ordering
        .try_reserve_exact(items.len())
        .map_err(|_| ApplyError::AllocationFailed)?;
    let mut finals = Vec::new();
    finals
        .try_reserve_exact(items.len())
        .map_err(|_| ApplyError::AllocationFailed)?;
    let fold = match run_independent(
        authority.batch(),
        &commands,
        limits,
        profile,
        |index, _, current, previous, bytes| {
            let item = *items.get(index).ok_or(ApplyError::InvalidAuthority)?;
            state.evaluate(item, current, previous, bytes)
        },
    )? {
        Ok(fold) => fold,
        Err(error) => {
            return Ok((
                IndependentIndexedDecision::Rejected(IndexRejection::Batch(error)),
                Vec::new(),
            ));
        }
    };
    let decision = finish(&state, items, fold, finals)?;
    Ok((decision, state.ordering))
}

fn finish<'current>(
    state: &IndependentIndexState<'_, 'current>,
    items: &[IndexedBatchItem<'_>],
    fold: IndependentResults<'_, IndexRejection>,
    mut finals: Vec<(usize, OrderingChange<'current>)>,
) -> Result<IndependentIndexedDecision<'current>, ApplyError> {
    let IndependentResults { results, latest } = fold;
    for (_, index) in latest {
        let item = items.get(index).ok_or(ApplyError::InvalidAuthority)?;
        let before = state.authority.current(item.item)?;
        let before_mode = before.map_or(Ok(StateMode::Parallel), |entry| {
            state.authority.mode(entry.record)
        })?;
        let (after, after_mode) = state
            .ordering
            .get(index)
            .copied()
            .flatten()
            .ok_or(ApplyError::InvalidAuthority)?;
        finals.push((
            index,
            OrderingChange {
                before,
                before_mode,
                after,
                after_mode,
            },
        ));
    }
    Ok(IndependentIndexedDecision::Accepted(
        IndependentIndexedPlan {
            results,
            finals,
            order: state.order,
            consumed: state.consumed,
        },
    ))
}
