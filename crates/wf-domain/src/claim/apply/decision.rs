//! Replay-approved physical selection and short-lived staged command composition.

use crate::claim::ScopedRecordId;
use std::collections::BTreeMap;
use wf_types::version::TimestampMillis;

use crate::claim::selection::{SelectionFailure, select_checked};
use crate::claim::{
    AuthoritativeClaimView, Claim, ClaimSelection, SelectionAction, SelectionError,
    SelectionRequest, SelectionStop, StateFilter,
};
use crate::value::Counter;
use crate::workflow::admission::{AdmissionError, AdmissionLimits};
use crate::workflow::batch::{ApplyError, BatchItem, BatchRejection};
use crate::workflow::indexed::{
    EntryOrder, IndexRejection, IndexedAuthority, IndexedBatchItem, IndexedBatchPlan,
    IndexedDecision, decide_selected_claims, decide_selected_claims_touched,
};
use crate::workflow::{CommittedCommand, DomainError, Operation, Outcome};

use super::admission::{
    ClaimPreflightError, admit_query, guard_consumed, guard_due, guard_lane, guard_scan,
    preflight_selected,
};
use super::result::ClaimResumeKind;
use super::token::source_token;
use super::{
    ClaimApplyView, ClaimDecision, ClaimEncoding, ClaimEnvelope, ClaimPlan, ClaimQuery,
    ClaimRejection, ClaimResume, ClaimStates, ClaimWorkLimits,
};

pub(crate) fn decide<'a>(
    view: ClaimApplyView<'a>,
    envelope: &ClaimEnvelope<'a>,
    now: TimestampMillis,
    limits: AdmissionLimits,
    work: ClaimWorkLimits,
) -> Result<ClaimDecision<'a>, ApplyError> {
    let attempt = (|| {
        let invalid = ClaimPreflightError::Apply(ApplyError::InvalidAuthority);
        if view.group.batch.group != view.gate.scope.group() || envelope.token_version != 1 {
            return Err(invalid);
        }
        let (selection, q, mut g) = select_phase(view, envelope, now, limits, work)?;
        preflight_selected(view, envelope, &selection, (now, q), limits, work, &mut g)?;
        let batch = if selection.records().is_empty() {
            None
        } else {
            Some(build_selected(view, envelope, &selection, now, limits)?)
        };
        let resume = if selection.stop() == SelectionStop::Complete {
            ClaimResume::complete()
        } else if batch.is_some() {
            ClaimResume::restart_from_current()
        } else {
            if selection.stop() != SelectionStop::ScanBudget || selection.scanned() == 0 {
                return Err(invalid);
            }
            ClaimResume::same_snapshot(
                view,
                envelope,
                now,
                limits,
                work,
                selection.continuation().ok_or(invalid)?,
            )
        };
        Ok(ClaimPlan::new(
            batch,
            selection.scanned(),
            selection.head_reads(),
            selection.stop(),
            resume,
            g,
        ))
    })();
    match attempt {
        Ok(plan) => Ok(ClaimDecision::Accepted(plan)),
        Err(ClaimPreflightError::Business(error)) => Ok(ClaimDecision::Rejected(error)),
        Err(ClaimPreflightError::Apply(error)) => Err(error),
    }
}

fn select_phase<'a>(
    view: ClaimApplyView<'a>,
    envelope: &ClaimEnvelope<'a>,
    now: TimestampMillis,
    limits: AdmissionLimits,
    work: ClaimWorkLimits,
) -> Result<(ClaimSelection<'a>, usize, u64), ClaimPreflightError> {
    let invalid = ClaimPreflightError::Apply(ApplyError::InvalidAuthority);
    let (q, mut g) = admit_query(envelope, now, limits, work)?;
    let cursor = if let Some(resume) = envelope.resume {
        let ClaimResumeKind::SameSnapshot(binding) = &resume.kind else {
            return Err(ClaimPreflightError::Business(
                ClaimRejection::StaleContinuation,
            ));
        };
        if binding.scope != view.gate.scope
            || binding.query != envelope.query
            || binding.token_version != envelope.token_version
            || binding.now != now
            || binding.revision != view.revision
            || binding.scan_budget != envelope.scan_budget
            || binding.head_budget != envelope.head_budget
            || binding.limits != limits
            || binding.work != work
        {
            return Err(ClaimPreflightError::Business(
                ClaimRejection::StaleContinuation,
            ));
        }
        Some(&binding.cursor)
    } else {
        None
    };
    let authority = AuthoritativeClaimView {
        group: view.gate.scope.group().as_bytes(),
        revision: view.revision,
        records: view.group.records,
        parallel: view.group.parallel,
        fifo: view.group.fifo,
    };
    let mut scanned = 0;
    let selection = select_checked(
        &authority,
        mapped_selection_request(view.gate.scope.group().as_bytes(), envelope, now),
        cursor,
        |record, lane, due| match (record, lane, due) {
            (Some(entry), None, None) => guard_consumed(view, entry, limits, work, &mut g),
            (None, Some(lane), None) => guard_lane(lane, limits, work, &mut g),
            (None, None, Some(due)) => guard_due(due, limits, work, &mut g),
            (None, None, None) => guard_scan(&mut scanned, &mut g, work),
            _ => Err(invalid),
        },
        |entry| physical_matches(envelope.query, entry),
    )
    .map_err(|error| match error {
        SelectionFailure::Guard(error) => error,
        SelectionFailure::Selection(SelectionError::HeadBudget) => {
            ClaimPreflightError::Business(ClaimRejection::HeadBudget)
        }
        SelectionFailure::Selection(SelectionError::StaleContinuation) => {
            ClaimPreflightError::Business(ClaimRejection::StaleContinuation)
        }
        SelectionFailure::Selection(SelectionError::InvalidScope) => {
            ClaimPreflightError::Business(ClaimRejection::InvalidScope)
        }
        SelectionFailure::Selection(SelectionError::InvalidBudget) => {
            ClaimPreflightError::Business(ClaimRejection::InvalidBudget)
        }
        SelectionFailure::Selection(SelectionError::AllocationFailed) => {
            ClaimPreflightError::Apply(ApplyError::AllocationFailed)
        }
        SelectionFailure::Selection(_) => invalid,
    })?;
    if scanned != selection.scanned() {
        return Err(invalid);
    }
    Ok((selection, q, g))
}

fn build_selected<'a>(
    view: ClaimApplyView<'a>,
    envelope: &ClaimEnvelope<'a>,
    selection: &ClaimSelection<'a>,
    now: TimestampMillis,
    limits: AdmissionLimits,
) -> Result<IndexedBatchPlan<'a>, ClaimPreflightError> {
    build_selected_with(
        (IndexedAuthority::Full(view.group), view.encodings),
        envelope,
        selection,
        now,
        limits,
    )
}

pub(super) fn build_selected_with<'current>(
    view: (
        IndexedAuthority<'_, 'current>,
        &BTreeMap<ScopedRecordId<'_>, ClaimEncoding<'_>>,
    ),
    envelope: &ClaimEnvelope<'_>,
    selection: &ClaimSelection<'current>,
    now: TimestampMillis,
    limits: AdmissionLimits,
) -> Result<IndexedBatchPlan<'current>, ClaimPreflightError> {
    let (view, encodings) = view;
    let invalid = ClaimPreflightError::Apply(ApplyError::InvalidAuthority);
    let mut commands = Vec::new();
    commands
        .try_reserve_exact(selection.records().len())
        .map_err(|_| ClaimPreflightError::Apply(ApplyError::AllocationFailed))?;
    for selected in selection.records() {
        let fence = selected.record.fence().checked_next().ok_or(invalid)?;
        let claim = Claim {
            expected_state: selected.record.state().to_vec(),
            worker: envelope.query.worker.to_vec(),
            token: source_token(envelope.query.worker, now, fence)?,
            lease_ms: envelope.query.lease_ms,
        };
        commands.push(CommittedCommand {
            id: selected.record.id().clone(),
            now,
            operation: match selected.action {
                SelectionAction::Claim => Operation::Claim(claim),
                SelectionAction::Reclaim => Operation::Reclaim(claim),
            },
        });
    }
    let mut items = Vec::new();
    items
        .try_reserve_exact(commands.len())
        .map_err(|_| ClaimPreflightError::Apply(ApplyError::AllocationFailed))?;
    for (selected, command) in selection.records().iter().zip(&commands) {
        let id = crate::claim::ScopedRecordId {
            partition_key: selected.partition_key,
            id: selected.record.id().as_bytes(),
        };
        let encoding = encodings.get(&id).ok_or(invalid)?;
        items.push(IndexedBatchItem::new(BatchItem::new(
            encoding.partition,
            command,
            encoding.state_key,
        )));
    }
    let order = EntryOrder {
        identity: envelope.header.identity.log(),
        first_ordinal: envelope.first_ordinal,
    };
    let decided = match view {
        IndexedAuthority::Full(view) => {
            decide_selected_claims(view, &items, order, envelope.header.encoded_entry, limits)
        }
        IndexedAuthority::Touched(view) => decide_selected_claims_touched(
            view,
            &items,
            order,
            envelope.header.encoded_entry,
            limits,
        ),
    };
    match decided.map_err(ClaimPreflightError::Apply)? {
        IndexedDecision::Accepted(batch) => {
            if batch.consumed_ordinals() != 0
                || batch.batch().items().len() != commands.len()
                || batch.batch().items().iter().any(|item| {
                    item.plan().outcome() != Outcome::Changed
                        || item.plan().write().is_none()
                        || item.plan().history().is_none()
                })
            {
                return Err(invalid);
            }
            Ok(batch)
        }
        IndexedDecision::Rejected(rejection) => Err(classify_index_rejection(rejection)),
    }
}

fn classify_index_rejection(rejection: IndexRejection) -> ClaimPreflightError {
    let invalid = ClaimPreflightError::Apply(ApplyError::InvalidAuthority);
    match rejection {
        IndexRejection::Batch(BatchRejection::Domain(error)) => match error {
            DomainError::CounterOverflow(Counter::Version) => {
                ClaimPreflightError::Business(ClaimRejection::VersionOverflow)
            }
            DomainError::CounterOverflow(Counter::Fence) => {
                ClaimPreflightError::Business(ClaimRejection::FenceOverflow)
            }
            DomainError::ZeroLease => ClaimPreflightError::Business(ClaimRejection::ZeroLease),
            DomainError::LeaseTooLarge => {
                ClaimPreflightError::Business(ClaimRejection::LeaseTooLarge)
            }
            DomainError::LeaseDeadlineTooLarge => {
                ClaimPreflightError::Business(ClaimRejection::LeaseDeadlineTooLarge)
            }
            _ => invalid,
        },
        IndexRejection::Batch(BatchRejection::Admission(error)) => match error {
            AdmissionError::ValueTooLarge { bytes, maximum } => {
                ClaimPreflightError::Business(ClaimRejection::ValueTooLarge { bytes, maximum })
            }
            AdmissionError::DecisionBytes => {
                ClaimPreflightError::Business(ClaimRejection::DecisionBytes)
            }
            AdmissionError::LengthOverflow => {
                ClaimPreflightError::Business(ClaimRejection::LengthOverflow)
            }
            _ => invalid,
        },
        _ => invalid,
    }
}

pub(super) fn mapped_selection_request<'a>(
    group: &'a [u8],
    envelope: &ClaimEnvelope<'a>,
    now: TimestampMillis,
) -> SelectionRequest<'a> {
    SelectionRequest {
        group,
        workflow_type: envelope.query.workflow_type,
        partitions: envelope.query.partitions,
        states: StateFilter::Any,
        exclude_states: &[],
        priority: envelope.query.priority,
        now,
        limit: envelope.query.limit,
        scan_budget: envelope.scan_budget,
        head_budget: envelope.head_budget,
        reclaim_expired: true,
    }
}

pub(super) fn physical_matches(
    query: ClaimQuery<'_>,
    entry: crate::claim::ClaimRecord<'_>,
) -> bool {
    (match query.states {
        ClaimStates::Any => true,
        ClaimStates::Only(states) => states.contains(&entry.record.state()),
    }) && !query.exclude_states.contains(&entry.record.state())
}
