//! Claim publication repeats the real guard profile before any durable change.

use super::super::store::{RedbStateTransaction, fence};
use super::{publication, validate_versions, write_control, write_outcome, write_records};
use crate::transaction::ClaimValuePublicationPlan;
use redb::Durability;
use wf_domain::workflow::control::plan_control;
use wf_domain::workflow::request::{RequestScope, RetainedOutcome};
use wf_ports::log::{
    ClaimApplyDecision, CodecError, claim_response_matches, claim_retained_summary,
};
use wf_ports::state::{GroupApplyTransaction, GroupClaimChanges, StateTransactionError};

mod governed;
pub(super) mod values;

pub(in super::super) fn apply(
    tx: RedbStateTransaction<'_>,
    changes: &GroupClaimChanges<'_, '_>,
) -> Result<(), StateTransactionError> {
    let fenced = tx.fenced;
    publish(tx, changes, None, None).inspect_err(|error| fence(fenced, *error))
}

pub(in super::super) fn apply_values(
    tx: RedbStateTransaction<'_>,
    changes: &GroupClaimChanges<'_, '_>,
    publication: &mut ClaimValuePublicationPlan<'_>,
) -> Result<(), StateTransactionError> {
    let fenced = tx.fenced;
    publish(tx, changes, Some(publication), None).inspect_err(|error| fence(fenced, *error))
}

pub(in super::super) fn apply_governed(
    tx: RedbStateTransaction<'_>,
    changes: &GroupClaimChanges<'_, '_>,
    publication: &mut ClaimValuePublicationPlan<'_>,
    paid: (
        wf_ports::log::CapturedGovernedClaimPayment<'_>,
        wf_domain::governance::credit::CreditLimits,
    ),
) -> Result<(), StateTransactionError> {
    let fenced = tx.fenced;
    publish(tx, changes, Some(publication), Some(paid)).inspect_err(|error| fence(fenced, *error))
}

fn publish(
    mut tx: RedbStateTransaction<'_>,
    changes: &GroupClaimChanges<'_, '_>,
    mut publication: Option<&mut ClaimValuePublicationPlan<'_>>,
    capture: Option<(
        wf_ports::log::CapturedGovernedClaimPayment<'_>,
        wf_domain::governance::credit::CreditLimits,
    )>,
) -> Result<(), StateTransactionError> {
    tx.ready()?;
    match publication.as_deref_mut() {
        Some(values) => values::validate(&tx, changes, values)?,
        None if matches!(tx.layout, 7 | 8) => return Err(StateTransactionError::UnsupportedLayout),
        None => {}
    }
    let scope = changes.scope();
    if scope.group() != tx.group || scope.recovery() != tx.recovery {
        return Err(StateTransactionError::InvalidChanges);
    }
    if tx.control()? != changes.expected() {
        return Err(StateTransactionError::KnownAborted);
    }
    validate(&tx, changes)?;
    let verified = governed::validate(&tx, changes, capture)?;
    let Some(control) = changes.control() else {
        return tx
            .write
            .abort()
            .map_err(|error| super::storage_error(&error));
    };
    if let Some(business) = changes.business() {
        validate_versions(&tx, scope, business)?;
        let lanes =
            publication::affected_lanes(&tx, business.deltas().len(), business.deltas().map(Ok))?;
        for lane in &lanes {
            publication::head_due(&tx, scope, *lane, false)?;
        }
        for delta in business.deltas() {
            publication::index_delta(&tx, scope, &delta)?;
        }
        match verified.as_ref() {
            Some(verified) => governed::write_records(&tx, changes, verified)?,
            None => write_records(&tx, scope, business, None)?,
        }
        for lane in lanes {
            publication::head_due(&tx, scope, lane, true)?;
        }
    }
    if let Some(values) = publication {
        values::publish(&tx, scope, values)?;
    }
    if let Some((capture, _)) = capture {
        super::super::release::claim::publish(&tx, changes, capture)?;
    }
    if let Some(outcome) = changes.outcome() {
        write_outcome(&tx, scope, outcome)?;
    }
    write_control(&tx, control)?;
    tx.write
        .set_durability(Durability::Immediate)
        .map_err(|_| StateTransactionError::Unavailable)?;
    tx.write
        .commit()
        .map_err(|_| StateTransactionError::Uncertain)
}

fn validate(
    tx: &RedbStateTransaction<'_>,
    changes: &GroupClaimChanges<'_, '_>,
) -> Result<(), StateTransactionError> {
    let plan = changes.plan();
    let control = plan.control();
    let next = control.next();
    let identity = next
        .last_entry()
        .ok_or(StateTransactionError::InvalidChanges)?;
    let stamp = next
        .last_entry_stamp()
        .ok_or(StateTransactionError::InvalidChanges)?;
    if plan_control(changes.expected(), identity, stamp)
        .map_err(|_| StateTransactionError::InvalidChanges)?
        != control
        || control.is_log_replay() != matches!(plan.decision(), ClaimApplyDecision::LogReplay)
    {
        return Err(StateTransactionError::InvalidChanges);
    }
    match plan.decision() {
        ClaimApplyDecision::Fresh { operation, outcome } => {
            validate_outcome(tx, changes.scope(), outcome)?;
            if outcome.original_position() != identity.log()
                || outcome.original_time() != control.execution_time()
                || !if outcome.intent().version() == 19 {
                    wf_ports::log::governed_claim_response_matches
                } else {
                    claim_response_matches
                }(
                    operation,
                    outcome.response(),
                    outcome.original_time(),
                    tx.admission,
                )
                .map_err(codec_error)?
            {
                return Err(StateTransactionError::InvalidChanges);
            }
        }
        ClaimApplyDecision::Replay(outcome) => validate_outcome(tx, changes.scope(), outcome)?,
        _ => {}
    }
    Ok(())
}

fn validate_outcome(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    outcome: &RetainedOutcome,
) -> Result<(), StateTransactionError> {
    if outcome.scope() != scope {
        return Err(StateTransactionError::InvalidChanges);
    }
    let (status, _) = if outcome.intent().version() == 19 {
        wf_ports::log::governed_claim_retained_summary
    } else {
        claim_retained_summary
    }(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
        outcome.original_time(),
        tx.admission,
    )
    .map_err(codec_error)?;
    if status != outcome.status() {
        return Err(StateTransactionError::InvalidChanges);
    }
    Ok(())
}

fn codec_error(error: CodecError) -> StateTransactionError {
    match error {
        CodecError::ByteLimit | CodecError::ItemLimit | CodecError::ValueLimit => {
            StateTransactionError::Limit
        }
        _ => StateTransactionError::InvalidChanges,
    }
}
