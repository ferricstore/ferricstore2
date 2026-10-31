//! One consumed writer publishes all identified rows, receipt and checkpoint.

use super::super::{
    commit,
    store::{RedbStateTransaction, fence},
};
use super::CreditReadContext;
use redb::Durability;
use wf_domain::governance::credit::CreditLimits;
use wf_domain::governance::identified::{IdentifiedCreditDecision as D, IdentifiedCreditPlan};
use wf_domain::workflow::control::{ControlPlan, ControlState, EntryIdentity};
use wf_domain::workflow::request::{RequestScope, RetainedOutcome, RetainedStatus};
use wf_ports::log::verify_credit_reply_parts;
use wf_ports::state::{GroupApplyTransaction, StateTransactionError as E};

mod rows;

pub(crate) fn apply(
    tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    expected: ControlState,
    control: ControlPlan,
    plan: Option<&IdentifiedCreditPlan<'_>>,
    outcome: Option<&RetainedOutcome>,
    credit: CreditLimits,
) -> Result<(), E> {
    let fenced = tx.fenced;
    publish(tx, scope, expected, control, plan, outcome, credit)
        .inspect_err(|error| fence(fenced, *error))
}

fn publish(
    mut tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    expected: ControlState,
    control: ControlPlan,
    plan: Option<&IdentifiedCreditPlan<'_>>,
    outcome: Option<&RetainedOutcome>,
    credit: CreditLimits,
) -> Result<(), E> {
    tx.ready()?;
    if scope.group() != tx.group || scope.recovery() != tx.recovery {
        return Err(E::InvalidChanges);
    }
    if tx.layout != 8 || tx.value_manifest()?.credit_shard_count.is_none() {
        return Err(E::UnsupportedPolicyProfile);
    }
    if tx.control()? != expected {
        return Err(E::KnownAborted);
    }
    if control.is_log_replay() {
        if plan.is_some() || outcome.is_some() {
            return Err(E::InvalidChanges);
        }
        return tx
            .write
            .abort()
            .map_err(|e| super::super::admission::storage_error(&e));
    }
    if let Some(plan) = plan {
        let outcome = outcome.ok_or(E::InvalidChanges)?;
        validate(&tx, scope, control, plan, outcome)?;
        let context = CreditReadContext::new(&tx, scope, plan.scope(), credit)?;
        super::catalog::publish(&context, plan)?;
        rows::publish(&context, plan)?;
        commit::write_outcome(&tx, scope, outcome)?;
    } else if outcome.is_some() {
        return Err(E::InvalidChanges);
    }
    commit::write_control(&tx, control.next())?;
    tx.write
        .set_durability(Durability::Immediate)
        .map_err(|_| E::Unavailable)?;
    tx.write.commit().map_err(|_| E::Uncertain)
}

fn validate(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    control: ControlPlan,
    plan: &IdentifiedCreditPlan<'_>,
    outcome: &RetainedOutcome,
) -> Result<(), E> {
    let status = match plan.decision() {
        D::Rejected(reason) => RetainedStatus::GovernanceRejected(reason),
        _ => RetainedStatus::Accepted,
    };
    if outcome.scope() != scope
        || !matches!(outcome.intent().version(), 12 | 17)
        || outcome.response_version() != 10
        || outcome.status() != status
        || Some(outcome.original_position()) != control.next().last_entry().map(EntryIdentity::log)
        || outcome.original_time() != control.execution_time()
    {
        return Err(E::InvalidChanges);
    }
    tx.temporary(18_432)?;
    verify_credit_reply_parts(
        plan.decision(),
        plan.owner(),
        plan.response_ids(),
        outcome.response(),
        18_432,
    )
    .map_err(|_| E::InvalidChanges)
}
