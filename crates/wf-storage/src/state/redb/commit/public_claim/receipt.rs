//! One exact public receipt and original control publication, with Immediate commit.
use super::{Limits, codec, validate_guard};
use crate::state::redb::{
    commit::atomic_claim::publish,
    store::{RedbStateTransaction, fence},
};
use wf_domain::workflow::{
    MAX_EXACT_MILLIS,
    control::{ControlPlan, ControlState},
    request::{RetainedMetadata, RetainedOutcome},
};
use wf_ports::{
    log::{PreparedAtomicClaim, public_claim_retained_summary},
    state::StateTransactionError as E,
};
use wf_types::version::TimestampMillis;

pub(super) fn finish(
    tx: RedbStateTransaction<'_>,
    prepared: &PreparedAtomicClaim<'_>,
    expected: ControlState,
    control: ControlPlan,
    response: Vec<u8>,
    limits: Limits,
) -> Result<(ControlState, Box<RetainedOutcome>), E> {
    validate_guard(&tx, expected, prepared, control)?;
    let time = control.execution_time();
    let (status, _) = public_claim_retained_summary(
        prepared.intent(),
        18,
        &response,
        time,
        limits.base.values.record_batch.codec.admission(),
    )
    .map_err(codec)?;
    let expiry = time
        .get()
        .checked_add(limits.base.values.record_batch.outcome_ttl_ms)
        .filter(|n| *n <= MAX_EXACT_MILLIS)
        .ok_or(E::Limit)?;
    let outcome = Box::new(
        RetainedOutcome::new(
            RetainedMetadata {
                scope: prepared.scope().clone(),
                request: prepared.request().clone(),
                intent: prepared.intent(),
                original_position: control.next().last_entry().ok_or(E::InvalidChanges)?.log(),
                original_time: time,
                expires_at: TimestampMillis::new(expiry),
                status,
            },
            18,
            response,
            limits.base.values.record_batch.codec.admission(),
        )
        .map_err(|_| E::InvalidChanges)?,
    );
    let fenced = tx.fenced;
    publish(tx, prepared, control.next(), Some(&outcome))
        .inspect_err(|error| fence(fenced, *error))?;
    Ok((control.next(), outcome))
}
