//! Repeat actual original authority, reindex once, then atomically publish layout6.

use super::super::{
    admission::total,
    core_policy::{self, catalog, reindex},
    original_policy::{original_error, original_owner},
    snapshot,
    store::{RedbStateTransaction, fence},
};
use super::{publication, write_control};
use crate::state::{key, value};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    original_record_batch_policy_projection_bytes, project_original_record_batch_policy,
    project_record_batch_policy, record_batch_policy_projection_bytes,
    stored_policy_indexed_fields_equal,
};
use wf_domain::policy::{InstalledRetryPolicy, definition::PolicySnapshot};
use wf_domain::workflow::control::plan_control;
use wf_domain::workflow::request::{DispatchDecision, RetainedOutcome};
use wf_ports::log::{
    CodecError, original_policy_set_response_matches, original_policy_set_response_status,
};
use wf_ports::state::{
    GroupApplyTransaction, GroupOriginalPolicyChanges, StateTransactionError as Error,
    StoredPolicyValue,
};

pub(in crate::state::redb) fn apply(
    tx: RedbStateTransaction<'_>,
    changes: &GroupOriginalPolicyChanges<'_>,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<u16, Error> {
    let fenced = tx.fenced;
    publish(tx, changes, schema, charge).inspect_err(|e| fence(fenced, *e))
}
fn publish(
    mut tx: RedbStateTransaction<'_>,
    changes: &GroupOriginalPolicyChanges<'_>,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<u16, Error> {
    tx.ready()?;
    let old_layout = tx.layout;
    let scope = changes.scope();
    if scope.group() != tx.group || scope.recovery() != tx.recovery {
        return Err(Error::InvalidChanges);
    }
    core_policy::validate_profile(&tx, schema, charge)?;
    if tx.control()? != changes.expected() {
        return Err(Error::KnownAborted);
    }
    validate(&tx, changes, schema, charge)?;
    if changes
        .plan()
        .used_history_max_events_limit()
        .is_some_and(|n| n != tx.limits.history_max_events_limit())
        || changes
            .plan()
            .used_migration_generations()
            .is_some_and(|pair| pair != (0, 0))
    {
        return Err(Error::InvalidChanges);
    }
    let Some(control) = changes.control() else {
        tx.write.abort().map_err(|_| Error::Unavailable)?;
        return Ok(old_layout);
    };
    let actual = actual_predecessor(&tx, changes, schema, charge)?;
    let mut layout = old_layout;
    if let Some(install) = changes.business() {
        if install.stored_generation() != actual.as_ref().map_or(0, |(_, generation)| *generation) {
            return Err(Error::InvalidChanges);
        }
        let kind = changes
            .plan()
            .workflow_type()
            .ok_or(Error::InvalidChanges)?;
        let before = actual.as_ref().map(|(value, _)| value);
        if let Some(StoredPolicyValue::Original(raw)) = before {
            tx.temporary(original_owner(raw.len())?)?;
        }
        tx.temporary(original_owner(install.encoded().len())?)?;
        let same = stored_policy_indexed_fields_equal(before, install.encoded(), schema, charge)
            .map_err(|e| match e {
                CompletePolicyEncodingError::Schema(PolicySchemaError::InvalidAuthority) => {
                    Error::UnsupportedPolicyEffects
                }
                other => original_error(other),
            })?;
        if !same {
            return Err(Error::UnsupportedPolicyEffects);
        }
        let old_projection = before
            .map(|value| stored_projection(&tx, value, kind, schema, charge))
            .transpose()?;
        let new_projection = projection(&tx, install.encoded(), kind, schema, charge)?;
        let saved = snapshot::policy::publication_catalog(&tx)?;
        if saved.pending.is_some() {
            return Err(Error::InvalidChanges);
        }
        let (actual_layout, manifest) = catalog::read(&tx)?;
        if actual_layout != old_layout {
            return Err(Error::Corrupt);
        }
        catalog::verify(&tx, actual_layout, manifest)?;
        let heads = reindex::scan_modes(
            &tx,
            scope,
            old_projection.as_ref().map(|(p, _)| p),
            &new_projection.0,
        )?;
        // Both owners are admitted before the one semantic status decode after true End.
        let outcome = changes.outcome().ok_or(Error::InvalidChanges)?;
        let encoded = encode_outcome(&tx, outcome, schema, charge)?;
        heads.finish_modes(&tx, scope, &new_projection.0)?;
        let key = tx.output_key(
            &[
                scope.group().as_bytes(),
                scope.recovery().as_bytes(),
                scope.tenant().as_bytes(),
                kind,
            ],
            0,
            install.encoded().len(),
            |cap| key::encode_policy_key(scope, kind, cap),
        )?;
        publication::put(&tx, &key, install.encoded())?;
        catalog::publish_original(&tx, old_layout, manifest, &saved)?;
        super::core_policy::put_outcome(&tx, scope, outcome, &encoded)?;
        layout = old_layout.max(snapshot::original::LAYOUT);
    } else {
        publish_no_business(&tx, changes, schema, charge)?;
    }
    write_control(&tx, control)?;
    tx.write
        .set_durability(redb::Durability::Immediate)
        .map_err(|_| Error::Unavailable)?;
    tx.write.commit().map_err(|_| Error::Uncertain)?;
    tx.committed_layout.set(layout);
    Ok(layout)
}
fn publish_no_business(
    tx: &RedbStateTransaction<'_>,
    changes: &GroupOriginalPolicyChanges<'_>,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    snapshot::policy::require_served(tx)?;
    if let Some(outcome) = changes.outcome() {
        let encoded = encode_outcome(tx, outcome, schema, charge)?;
        super::core_policy::put_outcome(tx, changes.scope(), outcome, &encoded)?;
    }
    Ok(())
}
fn actual_predecessor(
    tx: &RedbStateTransaction<'_>,
    changes: &GroupOriginalPolicyChanges<'_>,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<(StoredPolicyValue, u64)>, Error> {
    let actual = if matches!(changes.plan().decision(), DispatchDecision::Fresh(_)) {
        let kind = changes
            .plan()
            .workflow_type()
            .ok_or(Error::InvalidChanges)?;
        tx.read_stored_with_generation(changes.scope().tenant(), kind, schema, charge)?
    } else {
        None
    };
    if matches!(changes.plan().decision(), DispatchDecision::Fresh(_)) {
        let actual_raw = actual.as_ref().map(|(value, _)| value.encoded());
        if actual_raw != changes.predecessor() || !changes.plan().matches_predecessor(actual_raw) {
            return Err(Error::InvalidChanges);
        }
    }
    Ok(actual)
}
pub(super) fn projection(
    tx: &RedbStateTransaction<'_>,
    raw: &[u8],
    kind: &[u8],
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(PolicySnapshot, InstalledRetryPolicy), Error> {
    let owner = original_owner(raw.len())?;
    tx.temporary(owner)?;
    let bytes = original_record_batch_policy_projection_bytes(raw, kind, schema, charge)
        .map_err(original_error)?;
    tx.temporary(owner.checked_add(bytes).ok_or(Error::Limit)?)?;
    project_original_record_batch_policy(raw, kind, schema, charge, bytes).map_err(original_error)
}
fn validate(
    tx: &RedbStateTransaction<'_>,
    changes: &GroupOriginalPolicyChanges<'_>,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let plan = changes.plan();
    let control = plan.control();
    let next = control.next();
    let identity = next.last_entry().ok_or(Error::InvalidChanges)?;
    let stamp = next.last_entry_stamp().ok_or(Error::InvalidChanges)?;
    if plan.base_control() != changes.expected()
        || plan.expected_revision() != changes.expected().position().get()
        || plan_control(changes.expected(), identity, stamp).map_err(|_| Error::InvalidChanges)?
            != control
        || control.is_log_replay() != matches!(plan.decision(), DispatchDecision::LogReplay)
    {
        return Err(Error::InvalidChanges);
    }
    match plan.decision() {
        DispatchDecision::Fresh((install, outcome)) => {
            if plan.portable_profile_id() != Some(1)
                || outcome.scope() != changes.scope()
                || outcome.intent().version() != 4
                || outcome.original_position() != identity.log()
                || outcome.original_time() != control.execution_time()
            {
                return Err(Error::InvalidChanges);
            }
            let mut late = total(
                outcome.response().len(),
                [
                    outcome.scope().tenant().as_bytes().len(),
                    outcome.scope().group().as_bytes().len(),
                    outcome.scope().recovery().as_bytes().len(),
                    outcome.request().as_bytes().len(),
                ],
            )?;
            if let Some(install) = install {
                late = total(late, [install.policy_key().len(), install.encoded().len()])?;
            }
            if late > tx.admission.decision_bytes() {
                return Err(Error::Limit);
            }
            let owner = validation(outcome)?;
            tx.temporary(owner)?;
            if !original_policy_set_response_matches(plan, schema, charge, owner)
                .map_err(codec_error)?
            {
                return Err(Error::InvalidChanges);
            }
        }
        DispatchDecision::Replay(outcome) => {
            if outcome.scope() != changes.scope() {
                return Err(Error::InvalidChanges);
            }
            let owner = validation(outcome)?;
            tx.temporary(owner)?;
            if original_policy_set_response_status(outcome.response(), schema, charge, owner)
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
fn validation(outcome: &RetainedOutcome) -> Result<usize, Error> {
    if outcome.response().first() == Some(&131) {
        original_owner(outcome.response().len())
    } else {
        Ok(0)
    }
}
fn encode_outcome(
    tx: &RedbStateTransaction<'_>,
    outcome: &RetainedOutcome,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<u8>, Error> {
    let owner = validation(outcome)?;
    tx.temporary(owner)?;
    let scope = outcome.scope();
    let bytes = total(
        102,
        [
            scope.tenant().as_bytes().len(),
            scope.group().as_bytes().len(),
            scope.recovery().as_bytes().len(),
            outcome.request().as_bytes().len(),
            outcome.response().len(),
        ],
    )?;
    tx.temporary(bytes)?;
    value::encode_outcome_profiled(outcome, bytes, tx.admission, schema, charge, owner).map_err(
        |e| match e {
            value::StateValueError::Allocation => Error::Allocation,
            value::StateValueError::UnsupportedPolicyProfile => Error::UnsupportedPolicyProfile,
            value::StateValueError::ProfileLimit | value::StateValueError::Limit { .. } => {
                Error::Limit
            }
            _ => Error::InvalidChanges,
        },
    )
}
fn codec_error(e: CodecError) -> Error {
    match e {
        CodecError::Allocation => Error::Allocation,
        CodecError::ByteLimit | CodecError::ItemLimit | CodecError::ValueLimit => Error::Limit,
        CodecError::UnsupportedPolicyProfile => Error::UnsupportedPolicyProfile,
        _ => Error::InvalidChanges,
    }
}

fn stored_projection(
    tx: &RedbStateTransaction<'_>,
    value: &StoredPolicyValue,
    kind: &[u8],
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(PolicySnapshot, InstalledRetryPolicy), Error> {
    match value {
        StoredPolicyValue::Original(raw) => projection(tx, raw, kind, schema, charge),
        StoredPolicyValue::Complete(value) => {
            let bytes = record_batch_policy_projection_bytes(value).map_err(original_error)?;
            tx.temporary(bytes)?;
            project_record_batch_policy(value, schema, charge, bytes)
                .map(|(policy, retry)| (policy.into_policy(), retry))
                .map_err(original_error)
        }
    }
}
