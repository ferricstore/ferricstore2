//! One consumed Immediate writer publishes bounded ACK effects and original reply.

use super::super::super::{
    admission::{storage_error, table_error, value_error},
    commit, publication,
    store::{RedbStateTransaction, fence},
};
use super::{catalog_key, read_meta};
use crate::state::value;
use redb::Durability;
use wf_domain::{
    governance::catalog::{
        CreditCatalogAckPlan, CreditCatalogAckReply, CreditCatalogMeta, prepare_catalog_ack,
    },
    workflow::{
        control::{ControlPlan, ControlState, EntryIdentity},
        request::{RequestScope, RetainedOutcome, RetainedStatus},
    },
};
use wf_ports::{
    log::{catalog_ack_request_intent, verify_catalog_ack_reply},
    state::{GroupApplyTransaction, StateTransactionError as E},
};

pub(crate) fn current(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
) -> Result<CreditCatalogMeta, E> {
    tx.checked(|| {
        require(tx, scope)?;
        tx.step()?;
        let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
        Ok(read_meta(tx, &table, scope)?.unwrap_or(CreditCatalogMeta::empty()))
    })
}

pub(crate) fn apply(
    tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    expected: ControlState,
    control: ControlPlan,
    plan: Option<&CreditCatalogAckPlan>,
    outcome: Option<&RetainedOutcome>,
) -> Result<(), E> {
    let fenced = tx.fenced;
    publish(tx, scope, expected, control, plan, outcome).inspect_err(|error| fence(fenced, *error))
}

fn publish(
    mut tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    expected: ControlState,
    control: ControlPlan,
    plan: Option<&CreditCatalogAckPlan>,
    outcome: Option<&RetainedOutcome>,
) -> Result<(), E> {
    tx.ready()?;
    require(&tx, scope)?;
    if tx.control()? != expected {
        return Err(E::KnownAborted);
    }
    if control.is_log_replay() {
        if plan.is_some() || outcome.is_some() {
            return Err(E::InvalidChanges);
        }
        return tx.write.abort().map_err(|e| storage_error(&e));
    }
    if let Some(plan) = plan {
        let outcome = outcome.ok_or(E::InvalidChanges)?;
        validate(&tx, scope, control, plan, outcome)?;
        effects(&tx, scope, plan)?;
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

fn effects(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    plan: &CreditCatalogAckPlan,
) -> Result<(), E> {
    tx.step()?;
    let mut table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    let actual = read_meta(tx, &table, scope)?.unwrap_or(CreditCatalogMeta::empty());
    if actual != plan.original() {
        return Err(E::KnownAborted);
    }
    if prepare_catalog_ack(actual, plan.request()).reply() != plan.reply() {
        return Err(E::InvalidChanges);
    }
    let Some(ack) = plan.acknowledgement() else {
        return Ok(());
    };
    for sequence in ack.sequences() {
        let key = catalog_key(tx, scope, Some(sequence))?;
        super::read(tx, &table, &key, |bytes| {
            value::catalog::intent(bytes)
                .map(|_| ())
                .map_err(value_error)
        })?
        .ok_or(E::Corrupt)?;
        tx.step()?;
        if table
            .remove(key.as_slice())
            .map_err(|e| storage_error(&e))?
            .is_none()
        {
            return Err(E::Corrupt);
        }
    }
    drop(table);
    let key = catalog_key(tx, scope, None)?;
    tx.temporary(22)?;
    publication::put(tx, &key, &value::catalog::encode_meta(ack.next()))
}

fn require(tx: &RedbStateTransaction<'_>, scope: &RequestScope) -> Result<(), E> {
    if scope.group() != tx.group || scope.recovery() != tx.recovery {
        return Err(E::NamespaceConflict);
    }
    if tx.require_identified_credit_profile()?.0 != 2 {
        return Err(E::UnsupportedPolicyProfile);
    }
    Ok(())
}

fn validate(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    control: ControlPlan,
    plan: &CreditCatalogAckPlan,
    outcome: &RetainedOutcome,
) -> Result<(), E> {
    tx.temporary(1024)?;
    let status = match plan.reply() {
        CreditCatalogAckReply::Accepted { .. } => RetainedStatus::Accepted,
        CreditCatalogAckReply::Rejected { reason, .. } => {
            RetainedStatus::CreditCatalogRejected(reason)
        }
    };
    if outcome.scope() != scope
        || outcome.intent()
            != catalog_ack_request_intent(plan.request()).map_err(|_| E::InvalidChanges)?
        || outcome.response_version() != 11
        || outcome.status() != status
        || Some(outcome.original_position()) != control.next().last_entry().map(EntryIdentity::log)
        || outcome.original_time() != control.execution_time()
    {
        return Err(E::InvalidChanges);
    }
    verify_catalog_ack_reply(plan.reply(), outcome.response()).map_err(|_| E::InvalidChanges)
}
