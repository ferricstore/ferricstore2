//! Query-scoped reached facts; no bounded map is presented as complete authority.

use super::admission::{
    ClaimPreflightError, admit_query, admit_query_with_prior, guard_consumed_with, guard_due,
    guard_lane, guard_observed_lane, guard_scan, preflight_selected_with,
};
use super::decision::{build_selected_with, mapped_selection_request, physical_matches};
use super::{
    ClaimDecision, ClaimEncoding, ClaimEnvelope, ClaimPlan, ClaimRejection, ClaimResume,
    ClaimWorkLimits,
};
use crate::claim::selection::{Head, SelectionFailure, merge};
use crate::claim::{
    ClaimRecord, ClaimSelection, DueOrder, LaneKey, LaneOrder, ScopedRecordId,
    SelectionContinuation, SelectionError, SelectionStop,
};
use crate::policy::definition::StateMode;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;
use crate::workflow::indexed::{IndexedAuthority, TouchedIndexedGroupView};
use std::collections::{BTreeMap, BTreeSet};
use wf_types::version::TimestampMillis;
mod bindings;
use bindings::ClaimBindingCursor;

/// Observed eligible parallel frontier, independent of operational exhaustion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimWindowEnd {
    /// The actual relevant parallel stream ended under the current revision.
    RangeEnd,
    /// The paid, unconsumed B+1 relevant candidate is included.
    PrefixEnd,
}

/// Ordinary query-scoped observations supplied by one coherent reader.
///
/// Missing reached facts refuse. This mathematical value neither proves origin
/// nor grants completeness to the existing full-index APIs.
#[derive(Clone, Copy, Debug)]
pub struct ClaimObservedWindow<'query, 'current> {
    /// Explicit current/mode/index/placement observations.
    pub indexed: TouchedIndexedGroupView<'query, 'current>,
    /// Actual typed partition and encoded state key for each consumed point.
    pub encodings: &'query BTreeMap<ScopedRecordId<'query>, ClaimEncoding<'query>>,
    /// Ordered relevant due parallel prefix, including its paid lookahead.
    pub parallel: &'query [ClaimRecord<'current>],
    /// Actual stream end or included lookahead; never an operational budget end.
    pub parallel_end: ClaimWindowEnd,
    /// All relevant logical FIFO lanes, including finite explicit empty probes.
    pub head_lanes: &'query [LaneKey<'query>],
    /// False only when actual H+1 discovery proved the head budget insufficient.
    pub heads_complete: bool,
}

/// Runs full query business admission before any Fresh authority is loaded.
///
/// # Errors
/// Returns operational invalid authority separately from sealed business refusal.
pub fn preflight_observed_claim_query(
    envelope: &ClaimEnvelope<'_>,
    now: TimestampMillis,
    admission: AdmissionLimits,
    work: ClaimWorkLimits,
) -> Result<Result<(usize, u64), ClaimRejection>, ApplyError> {
    if envelope.token_version != 1 || envelope.resume.is_some() {
        return Err(ApplyError::InvalidAuthority);
    }
    match admit_query(envelope, now, admission, work) {
        Ok(admitted) => Ok(Ok(admitted)),
        Err(ClaimPreflightError::Business(error)) => Ok(Err(error)),
        Err(ClaimPreflightError::Apply(error)) => Err(error),
    }
}

/// Reduces reached facts after the point gate, retaining only current borrows.
///
/// Query admission runs again and cumulatively pays `prior_work`; a tuple from
/// preflight never bypasses validation. Partial results advise a fresh restart.
/// # Errors
/// Refuses missing/incoherent facts, unsupported token/cursor or host allocation.
pub fn decide_observed_claim<'current>(
    window: ClaimObservedWindow<'_, 'current>,
    envelope: &ClaimEnvelope<'_>,
    now: TimestampMillis,
    admission: AdmissionLimits,
    work: ClaimWorkLimits,
    prior_work: u64,
) -> Result<ClaimDecision<'current>, ApplyError> {
    match attempt(window, envelope, now, admission, work, prior_work, None) {
        Ok(plan) => Ok(ClaimDecision::Accepted(plan)),
        Err(ClaimPreflightError::Business(error)) => Ok(ClaimDecision::Rejected(error)),
        Err(ClaimPreflightError::Apply(error)) => Err(error),
    }
}

/// Reduces the public source profile with bind-before-skip accounting.
/// `maximum_bindings` bounds remaining original IDs; it grants no credit authority.
/// Only the public source profile applies its query-row integer restriction.
/// # Errors
/// Refuses incoherent facts and allocation separately from sealed business refusal.
pub fn decide_observed_public_claim<'current>(
    window: ClaimObservedWindow<'_, 'current>,
    envelope: &ClaimEnvelope<'_>,
    now: TimestampMillis,
    admission: AdmissionLimits,
    work: ClaimWorkLimits,
    prior_work: u64,
    maximum_bindings: Option<usize>,
) -> Result<ClaimDecision<'current>, ApplyError> {
    match attempt(
        window,
        envelope,
        now,
        admission,
        work,
        prior_work,
        Some(maximum_bindings.unwrap_or(envelope.scan_budget)),
    ) {
        Ok(plan) => Ok(ClaimDecision::Accepted(plan)),
        Err(ClaimPreflightError::Business(error)) => Ok(ClaimDecision::Rejected(error)),
        Err(ClaimPreflightError::Apply(error)) => Err(error),
    }
}

fn attempt<'current>(
    window: ClaimObservedWindow<'_, 'current>,
    envelope: &ClaimEnvelope<'_>,
    now: TimestampMillis,
    limits: AdmissionLimits,
    work: ClaimWorkLimits,
    prior_work: u64,
    source_maximum: Option<usize>,
) -> Result<ClaimPlan<'current>, ClaimPreflightError> {
    if window.indexed.group != envelope.header.scope.group()
        || envelope.token_version != 1
        || envelope.resume.is_some()
    {
        return Err(ClaimPreflightError::Apply(ApplyError::InvalidAuthority));
    }
    let (mut q, mut g) = admit_query_with_prior(envelope, now, limits, work, prior_work)?;
    if !window.heads_complete || window.head_lanes.len() > envelope.head_budget {
        return Err(ClaimPreflightError::Business(ClaimRejection::HeadBudget));
    }
    let mut bindings = source_maximum
        .map(|maximum| {
            ClaimBindingCursor::new(maximum, envelope.query.limit, &mut q, &mut g, limits, work)
        })
        .transpose()?;
    let selection = select(
        window,
        envelope,
        now,
        limits,
        work,
        &mut g,
        bindings.as_mut(),
    )?;
    let authority = (IndexedAuthority::Touched(window.indexed), window.encodings);
    preflight_selected_with(
        authority,
        envelope,
        &selection,
        (now, q),
        limits,
        work,
        &mut g,
    )?;
    let batch = if selection.records().is_empty() {
        None
    } else {
        Some(build_selected_with(
            authority, envelope, &selection, now, limits,
        )?)
    };
    let resume = if selection.stop() == SelectionStop::Complete {
        ClaimResume::complete()
    } else {
        ClaimResume::restart_from_current()
    };
    Ok(ClaimPlan::new(
        batch,
        selection.scanned(),
        selection.head_reads(),
        selection.stop(),
        resume,
        g,
    )
    .with_source_bindings(bindings.map(ClaimBindingCursor::into_ledger)))
}

fn select<'query, 'current: 'query>(
    window: ClaimObservedWindow<'query, 'current>,
    envelope: &ClaimEnvelope<'_>,
    now: TimestampMillis,
    limits: AdmissionLimits,
    work: ClaimWorkLimits,
    g: &mut u64,
    mut bindings: Option<&mut ClaimBindingCursor>,
) -> Result<ClaimSelection<'current>, ClaimPreflightError> {
    let invalid = ClaimPreflightError::Apply(ApplyError::InvalidAuthority);
    if window.parallel.len() > envelope.scan_budget + 1
        || (window.parallel_end == ClaimWindowEnd::PrefixEnd
            && window.parallel.len() != envelope.scan_budget + 1)
    {
        return Err(invalid);
    }
    let request = mapped_selection_request(window.indexed.group.as_bytes(), envelope, now);
    let heads = heads(window, envelope, now, limits, work, g)?;
    let mut previous = None;
    for entry in window.parallel {
        let due = DueOrder::for_record(entry).ok_or(invalid)?;
        if !request.matches(*entry)
            || !physical_matches(envelope.query, *entry)
            || due.due > now
            || previous.is_some_and(|before| before >= due)
        {
            return Err(invalid);
        }
        previous = Some(due);
    }
    let mut cursor = SelectionContinuation {
        group: request.group,
        revision: 0,
        request,
        parallel_after: None,
        fifo_after: None,
    };
    let authority = IndexedAuthority::Touched(window.indexed);
    let mut scanned = 0;
    let (records, count, stop) = merge(
        &mut cursor,
        window
            .parallel
            .iter()
            .filter_map(|entry| DueOrder::for_record(entry).map(|due| (due, *entry))),
        heads.into_iter(),
        &mut |entry, lane, due| match (entry, lane, due) {
            (Some(entry), None, None) => {
                guard_consumed_with((authority, window.encodings), entry, limits, work, g)
            }
            (None, None, Some(due)) => guard_due(due, limits, work, g),
            (None, None, None) => guard_scan(&mut scanned, g, work),
            _ => Err(invalid),
        },
        |_| true,
        |due, entry| {
            if authority
                .mode(entry.record)
                .map_err(|_| SelectionError::InvalidIndex)?
                != StateMode::Parallel
                || !authority
                    .parallel_member(due, entry)
                    .map_err(|_| SelectionError::InvalidIndex)?
            {
                return Err(SelectionError::InvalidIndex);
            }
            Ok(())
        },
        |entry| {
            bindings.as_mut().map_or(Ok(true), |cursor| {
                cursor.bind(entry.record, envelope.query, now)
            })
        },
    )
    .map_err(|error| failure(&error))?;
    if scanned != count {
        return Err(invalid);
    }
    Ok(ClaimSelection {
        records,
        scanned,
        head_reads: window.head_lanes.len(),
        stop,
        continuation: None,
    })
}

fn heads<'current>(
    window: ClaimObservedWindow<'_, 'current>,
    envelope: &ClaimEnvelope<'_>,
    now: TimestampMillis,
    limits: AdmissionLimits,
    work: ClaimWorkLimits,
    g: &mut u64,
) -> Result<Vec<Head<'current>>, ClaimPreflightError> {
    let invalid = ClaimPreflightError::Apply(ApplyError::InvalidAuthority);
    let mut heads = Vec::new();
    heads
        .try_reserve_exact(window.head_lanes.len())
        .map_err(|_| ClaimPreflightError::Apply(ApplyError::AllocationFailed))?;
    let mut lanes = BTreeSet::new();
    let mut identities = BTreeSet::new();
    let request = mapped_selection_request(window.indexed.group.as_bytes(), envelope, now);
    for lane in window.head_lanes {
        guard_lane(*lane, limits, work, g)?;
        guard_observed_lane(window.indexed, *lane, envelope.query, limits)?;
        if !lanes.insert(*lane) {
            return Err(invalid);
        }
        let head = window.indexed.fifo_heads.get(lane).ok_or(invalid)?;
        let Some((order, entry)) = head else {
            continue;
        };
        guard_consumed_with(
            (IndexedAuthority::Touched(window.indexed), window.encodings),
            *entry,
            limits,
            work,
            g,
        )?;
        if LaneKey::for_record(entry) != Some(*lane)
            || LaneOrder::for_record(entry) != *order
            || !identities.insert((entry.partition_key, entry.record.id().as_bytes()))
        {
            return Err(invalid);
        }
        if let Some(order) = DueOrder::for_record(entry)
            && order.due <= now
            && request.matches(*entry)
            && physical_matches(envelope.query, *entry)
        {
            heads.push(Head {
                order,
                entry: *entry,
            });
        }
    }
    heads.sort_unstable_by_key(|head| head.order);
    Ok(heads)
}

fn failure(error: &SelectionFailure<ClaimPreflightError>) -> ClaimPreflightError {
    match error {
        SelectionFailure::Guard(error) => *error,
        SelectionFailure::Selection(SelectionError::AllocationFailed) => {
            ClaimPreflightError::Apply(ApplyError::AllocationFailed)
        }
        SelectionFailure::Selection(_) => ClaimPreflightError::Apply(ApplyError::InvalidAuthority),
    }
}
