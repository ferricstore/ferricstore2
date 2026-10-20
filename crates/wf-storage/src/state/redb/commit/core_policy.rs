//! Repeated supplied-change checks and one atomic complete policy publication.

use super::super::{
    admission::{storage_error, total},
    core_policy::{self, catalog, reindex},
    snapshot,
    store::{RedbStateTransaction, fence},
};
use super::{publication, write_control};
use crate::state::{key, value};
use wf_domain::policy::complete::{CompletePolicyValue, PolicySchemaCharge};
use wf_domain::workflow::control::plan_control;
use wf_domain::workflow::request::{DispatchDecision, RequestScope, RetainedOutcome};
use wf_ports::log::{
    CodecError, complete_core_policy_patch_response_matches,
    complete_core_policy_patch_retained_status,
};
use wf_ports::state::{
    GroupApplyTransaction, GroupCorePolicyChanges, StateTransactionError as Error,
};

pub(in crate::state::redb) fn apply(
    tx: RedbStateTransaction<'_>,
    changes: &GroupCorePolicyChanges<'_, '_, '_>,
    charge: &mut PolicySchemaCharge,
) -> Result<u16, Error> {
    let fenced = tx.fenced;
    publish(tx, changes, charge).inspect_err(|e| fence(fenced, *e))
}
fn publish(
    mut tx: RedbStateTransaction<'_>,
    changes: &GroupCorePolicyChanges<'_, '_, '_>,
    charge: &mut PolicySchemaCharge,
) -> Result<u16, Error> {
    tx.ready()?;
    let old_layout = tx.layout;
    let scope = changes.scope();
    if scope.group() != tx.group || scope.recovery() != tx.recovery {
        return Err(Error::InvalidChanges);
    }
    core_policy::validate_profile(&tx, charge.profile(), charge)?;
    if tx.control()? != changes.expected() {
        return Err(Error::KnownAborted);
    }
    validate(&tx, changes, charge)?;
    if changes
        .plan()
        .used_history_max_events_limit()
        .is_some_and(|used| used != tx.limits.history_max_events_limit())
    {
        return Err(Error::InvalidChanges);
    }
    let Some(control) = changes.control() else {
        tx.write.abort().map_err(|e| storage_error(&e))?;
        return Ok(old_layout);
    };
    let mut layout = old_layout;
    let current = if matches!(changes.plan().decision(), DispatchDecision::Fresh(_)) {
        let kind = changes
            .plan()
            .workflow_type()
            .ok_or(Error::InvalidChanges)?;
        let current = tx.installed_policy(scope.tenant(), kind, charge.profile(), charge)?;
        if current.as_ref().map(CompletePolicyValue::encoded)
            != changes.predecessor().map(CompletePolicyValue::encoded)
            || !changes.plan().matches_predecessor(current.as_ref())
        {
            return Err(Error::InvalidChanges);
        }
        current
    } else {
        None
    };
    if let Some((expected, candidate)) = changes.business() {
        if current.as_ref().map_or(0, CompletePolicyValue::generation) != *expected {
            return Err(Error::InvalidChanges);
        }
        let saved = snapshot::policy::publication_catalog(&tx)?;
        if saved.pending.is_some() {
            return Err(Error::InvalidChanges);
        }
        let (actual_layout, manifest) = catalog::read(&tx)?;
        if actual_layout != old_layout {
            return Err(Error::Corrupt);
        }
        catalog::verify(&tx, actual_layout, manifest)?;
        let heads = reindex::scan(&tx, scope, current.as_ref(), candidate)?;
        // This required encoding is the independent schema witness AFTER true End.
        // Its original wire stays owned through heads/catalog and is never re-encoded.
        let outcome = changes.outcome().ok_or(Error::InvalidChanges)?;
        let encoded = encode_outcome(&tx, outcome, charge)?;
        heads.finish(&tx, scope, candidate)?;
        let key = tx.output_key(
            &[
                scope.group().as_bytes(),
                scope.recovery().as_bytes(),
                scope.tenant().as_bytes(),
                &candidate.data().workflow_type,
            ],
            0,
            candidate.encoded().len(),
            |cap| key::encode_policy_key(scope, &candidate.data().workflow_type, cap),
        )?;
        publication::put(&tx, &key, candidate.encoded())?;
        catalog::publish(&tx, old_layout, manifest, &saved)?;
        put_outcome(&tx, scope, outcome, &encoded)?;
        layout = old_layout.max(catalog::POLICY_LAYOUT);
    } else {
        snapshot::policy::require_served(&tx)?;
        if let Some(outcome) = changes.outcome() {
            let encoded = encode_outcome(&tx, outcome, charge)?;
            put_outcome(&tx, scope, outcome, &encoded)?;
        }
    }
    write_control(&tx, control)?;
    tx.write
        .set_durability(redb::Durability::Immediate)
        .map_err(|_| Error::Unavailable)?;
    tx.write.commit().map_err(|_| Error::Uncertain)?;
    tx.committed_layout.set(layout);
    Ok(layout)
}
fn validate(
    tx: &RedbStateTransaction<'_>,
    changes: &GroupCorePolicyChanges<'_, '_, '_>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let plan = changes.plan();
    let control = plan.control();
    let identity = control.next().last_entry().ok_or(Error::InvalidChanges)?;
    let stamp = control
        .next()
        .last_entry_stamp()
        .ok_or(Error::InvalidChanges)?;
    if plan.base_control() != changes.expected()
        || plan_control(changes.expected(), identity, stamp).map_err(|_| Error::InvalidChanges)?
            != control
        || control.is_log_replay() != matches!(plan.decision(), DispatchDecision::LogReplay)
    {
        return Err(Error::InvalidChanges);
    }
    match plan.decision() {
        DispatchDecision::Fresh(fresh) => {
            let outcome = &fresh.1;
            if outcome.scope() != changes.scope()
                || outcome.original_position() != identity.log()
                || outcome.original_time() != control.execution_time()
            {
                return Err(Error::InvalidChanges);
            }
            let n = validation_bytes(outcome)?;
            tx.temporary(n)?;
            if !complete_core_policy_patch_response_matches(
                fresh,
                (charge.profile(), tx.admission),
                charge,
                n,
            )
            .map_err(codec_error)?
            {
                return Err(Error::InvalidChanges);
            }
        }
        DispatchDecision::Replay(outcome) => {
            if outcome.scope() != changes.scope() {
                return Err(Error::InvalidChanges);
            }
            let n = validation_bytes(outcome)?;
            tx.temporary(n)?;
            if complete_core_policy_patch_retained_status(
                outcome.intent(),
                outcome.response_version(),
                outcome.response(),
                (charge.profile(), tx.admission),
                charge,
                n,
            )
            .map_err(codec_error)?
                != outcome.status()
            {
                return Err(Error::InvalidChanges);
            }
        }
        _ => {}
    }
    Ok(())
}
fn validation_bytes(outcome: &RetainedOutcome) -> Result<usize, Error> {
    if outcome.response().starts_with(b"FWPV") {
        outcome
            .response()
            .len()
            .checked_mul(216)
            .and_then(|n| n.checked_add(2048))
            .ok_or(Error::Limit)
    } else {
        Ok(0)
    }
}
fn encode_outcome(
    tx: &RedbStateTransaction<'_>,
    outcome: &RetainedOutcome,
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<u8>, Error> {
    let validation = validation_bytes(outcome)?;
    tx.temporary(validation)?;
    let scope = outcome.scope();
    let n = total(
        102,
        [
            scope.group().as_bytes().len(),
            scope.recovery().as_bytes().len(),
            scope.tenant().as_bytes().len(),
            outcome.request().as_bytes().len(),
            outcome.response().len(),
        ],
    )?;
    tx.temporary(n)?;
    value::encode_outcome_profiled(
        outcome,
        n,
        tx.admission,
        charge.profile(),
        charge,
        validation,
    )
    .map_err(|e| match e {
        value::StateValueError::ProfileLimit | value::StateValueError::Limit { .. } => Error::Limit,
        value::StateValueError::Allocation => Error::Allocation,
        _ => Error::InvalidChanges,
    })
}
pub(super) fn put_outcome(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    outcome: &RetainedOutcome,
    encoded: &[u8],
) -> Result<(), Error> {
    let key = tx.output_key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            outcome.request().as_bytes(),
        ],
        0,
        encoded.len(),
        |cap| key::encode_outcome_key(scope, outcome.request(), cap),
    )?;
    publication::put(tx, &key, encoded)
}
fn codec_error(error: CodecError) -> Error {
    match error {
        CodecError::ByteLimit | CodecError::ItemLimit | CodecError::ValueLimit => Error::Limit,
        CodecError::Allocation => Error::Allocation,
        _ => Error::InvalidChanges,
    }
}
