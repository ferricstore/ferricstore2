//! Repeats captured-input correspondence and actual fresh paid eligibility.
use super::super::super::{
    history_snapshot, publication,
    release::claim::{self, VerifiedGovernedClaimWrites},
    store::RedbStateTransaction,
};
use wf_domain::{
    claim::apply::{ClaimDecision as D, ClaimRejection as R},
    workflow::request::RetainedStatus,
};
use wf_ports::{
    log::{
        CapturedGovernedClaimPayment as C, ClaimApplyDecision as A, inspect_governed_claim_reply,
    },
    state::{GroupClaimChanges, StateTransactionError as E},
};

pub(super) fn validate<'a>(
    tx: &RedbStateTransaction<'_>,
    changes: &'a GroupClaimChanges<'_, '_>,
    capture: Option<C<'_>>,
) -> Result<Option<VerifiedGovernedClaimWrites<'a>>, E> {
    let Some(capture) = capture else {
        let outcome = match changes.plan().decision() {
            A::Fresh { outcome, .. } => Some(outcome.as_ref()),
            A::Replay(outcome) => Some(*outcome),
            _ => None,
        };
        if outcome.is_some_and(|outcome| outcome.intent().version() == 19) {
            return Err(E::UnsupportedPolicyProfile);
        }
        return Ok(None);
    };
    tx.require_governed_claim()?;
    if changes
        .plan()
        .control()
        .next()
        .last_entry()
        .is_none_or(|entry| entry.prepared() != capture.prepared())
    {
        return Err(E::InvalidChanges);
    }
    let A::Fresh { operation, outcome } = changes.plan().decision() else {
        return Ok(None);
    };
    if outcome.intent().version() != 19 || outcome.response_version() != 16 {
        return Err(E::InvalidChanges);
    }
    tx.temporary(
        capture
            .payment()
            .count()
            .checked_mul(std::mem::size_of::<&[u8]>())
            .and_then(|n| n.checked_add(256))
            .ok_or(E::Limit)?,
    )?;
    let reply =
        inspect_governed_claim_reply(outcome.response(), outcome.original_time(), tx.admission)
            .map_err(super::codec_error)?;
    if !reply.payment().matches(capture.payment()) {
        return Err(E::InvalidChanges);
    }
    match operation {
        D::Accepted(_) => {
            if reply.status() != RetainedStatus::Accepted {
                return Err(E::InvalidChanges);
            }
            Ok(Some(claim::witness(tx, changes, capture)?))
        }
        D::Rejected(R::GoverningReservationUnavailable) => {
            if claim::available(
                tx,
                changes.scope(),
                capture,
                changes.plan().control().execution_time(),
                capture.requested_limit(),
            )? {
                return Err(E::InvalidChanges);
            }
            Ok(None)
        }
        D::Rejected(_) => Ok(None),
    }
}
pub(super) fn write_records(
    tx: &RedbStateTransaction<'_>,
    changes: &GroupClaimChanges<'_, '_>,
    verified: &VerifiedGovernedClaimWrites<'_>,
) -> Result<(), E> {
    let business = changes.business().ok_or(E::InvalidChanges)?;
    for (item, ordering) in business.batch().items().iter().zip(business.metadata()) {
        let write = item.plan().write().ok_or(E::InvalidChanges)?;
        publication::write_governed_claim_record(
            tx,
            changes.scope(),
            item.partition(),
            &write.record,
            ordering,
            verified,
        )?;
        let history = item.plan().history().ok_or(E::InvalidChanges)?;
        history_snapshot::write(
            tx,
            changes.scope(),
            item.partition(),
            &write.record,
            ordering,
            history,
        )?;
    }
    Ok(())
}
