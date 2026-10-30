//! Immutable values, aliases and every companion share the consumed core writer.

use sha2::{Digest, Sha256};
use wf_domain::history::EventKind;
use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::{
    OriginalValuePartition, OriginalValueRefKind, original_binary_stored_len,
    original_binary_stored_parts, render_original_value_ref,
};
use wf_ports::state::{GroupApplyChanges, StateTransactionError as Error};

use super::super::{
    admission,
    store::{RedbStateTransaction, fence},
};
use crate::state::redb::values::{
    budget::{ValueCharge, product, total},
    read::{build_error, scope_bytes},
};
use crate::state::{
    key,
    value::{self, ValueAlias},
};
use crate::transaction::{ValuePublicationPlan, ValueStep};

mod immutable;
use immutable::{immutable_value, key_bound, put, write_exact};

pub(crate) fn apply(
    tx: RedbStateTransaction<'_>,
    changes: &GroupApplyChanges<'_, '_>,
    publication: &mut ValuePublicationPlan<'_>,
) -> Result<(), Error> {
    let fenced = tx.fenced;
    super::publish(tx, changes, Some(publication)).inspect_err(|error| fence(fenced, *error))
}

pub(super) fn validate(
    tx: &RedbStateTransaction<'_>,
    changes: &GroupApplyChanges<'_, '_>,
    publication: &mut ValuePublicationPlan<'_>,
) -> Result<(), Error> {
    publication.charge.helper()?;
    if !matches!(tx.layout, 7 | 8) {
        return Err(Error::UnsupportedLayout);
    }
    if publication.intent_version == 9 && tx.value_manifest()?.alias_format != 2 {
        return Err(Error::UnsupportedPolicyProfile);
    }
    let step = tx.layout == 8 && publication.intent_version == 10 && publication.mode == 10;
    if !matches!(publication.intent_version, 8 | 9) && !step {
        return Err(Error::InvalidChanges);
    }
    if (!matches!(publication.mode, 1 | 5 | 6 | 7) && !step)
        || changes
            .plan()
            .control()
            .next()
            .last_entry()
            .is_none_or(|entry| entry.prepared() != publication.prepared)
    {
        return Err(Error::InvalidChanges);
    }
    if let Some(outcome) = changes.outcome() {
        let response = match publication.mode {
            1 => 8,
            5 => 5,
            6 => 6,
            7 => 7,
            10 if step => 9,
            _ => return Err(Error::InvalidChanges),
        };
        if outcome.intent().version() != publication.intent_version
            || outcome.response_version() != response
        {
            return Err(Error::InvalidChanges);
        }
    }
    let effects = changes
        .plan()
        .value_effects()
        .map_or(&[][..], wf_ports::blob::RecordBatchValueEffects::steps);
    if effects.len() != publication.steps.len() {
        return Err(Error::InvalidChanges);
    }
    let mut previous = None;
    publication
        .charge
        .work(product(publication.steps.len(), 128)?)?;
    for (step, effect) in publication.steps.iter().zip(effects) {
        publication.charge.candidate()?;
        let history = step.plan.plan().history().ok_or(Error::InvalidChanges)?;
        if step.source_index != effect.item_index()
            || step.supplied != effect.supplied()
            || history.version != effect.version()
            || previous.is_some_and(|old| old >= step.source_index)
            || (history.kind == EventKind::Created
                && step.binding.lifecycle.incarnation != step.ordering.state_enter_seq)
        {
            return Err(Error::InvalidChanges);
        }
        previous = Some(step.source_index);
    }
    Ok(())
}

pub(super) fn publish(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    publication: &mut ValuePublicationPlan<'_>,
) -> Result<(), Error> {
    publication.charge.helper()?;
    let live = publication.charge.peak();
    publication
        .charge
        .work(product(publication.steps.len(), 128)?)?;
    for step in &publication.steps {
        publication.charge.candidate()?;
        let record = &step
            .plan
            .plan()
            .write()
            .ok_or(Error::InvalidChanges)?
            .record;
        let fields = [record.payload(), record.result(), record.error()];
        for (index, field) in fields.into_iter().enumerate() {
            publication.charge.candidate()?;
            if step.supplied[index] {
                publish_value(
                    tx,
                    scope,
                    step,
                    (
                        index,
                        field.ok_or(Error::InvalidChanges)?,
                        publication.intent_version == 9,
                    ),
                    &mut publication.charge,
                    live,
                )?;
            }
        }
        publish_binding(tx, scope, step, false, &mut publication.charge, live)?;
        if step.final_write {
            publish_binding(tx, scope, step, true, &mut publication.charge, live)?;
        }
    }
    Ok(())
}

fn publish_value(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    step: &ValueStep<'_>,
    field: (usize, &[u8], bool),
    charge: &mut ValueCharge,
    live: usize,
) -> Result<(), Error> {
    charge.helper()?;
    let record = &step
        .plan
        .plan()
        .write()
        .ok_or(Error::InvalidChanges)?
        .record;
    let kind = match field.0 {
        0 => OriginalValueRefKind::Payload,
        1 => OriginalValueRefKind::Result,
        _ => OriginalValueRefKind::Error,
    };
    let partition = step.plan.partition().as_bytes();
    let id = record.id().as_bytes();
    charge.work(total(
        128,
        [
            product(total(partition.len(), [id.len()])?, 4)?,
            field.1.len(),
        ],
    )?)?;
    let reference_bound = total(128, [product(id.len(), 2)?])?;
    charge.temporary(total(live, [reference_bound])?)?;
    let reference = render_original_value_ref(
        id,
        kind,
        record.version(),
        OriginalValuePartition::Explicit(partition),
        65_535,
    )
    .map_err(|error| build_error(&error))?;
    let stored_len = if field.2 {
        field.1.len()
    } else {
        original_binary_stored_len(field.1, usize::MAX).map_err(|error| build_error(&error))?
    };
    let parts = if field.2 {
        [b"".as_slice(), field.1]
    } else {
        original_binary_stored_parts(field.1, usize::MAX).map_err(|error| build_error(&error))?
    };
    charge.work(stored_len)?;
    let mut hash = Sha256::new();
    hash.update(parts[0]);
    hash.update(parts[1]);
    let alias = ValueAlias {
        stored: field.2,
        partition,
        id,
        kind,
        version: record.version(),
        incarnation: step.binding.lifecycle.incarnation,
        expires_at: step.binding.lifecycle.terminal_deadline,
        stored_len: u64::try_from(stored_len).map_err(|_| Error::Limit)?,
        digest: hash.finalize().into(),
    };
    let key_bound = key_bound(scope, reference.len(), 16)?;
    charge.work(total(
        128,
        [product(total(scope_bytes(scope), [reference.len()])?, 32)?],
    )?)?;
    charge.temporary(total(
        live,
        [
            reference.len(),
            product(key_bound, 2)?,
            stored_len,
            119,
            partition.len(),
            id.len(),
        ],
    )?)?;
    let immutable = tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            &reference,
        ],
        16,
        |maximum| key::encode_immutable_key(scope, &reference, alias.incarnation, maximum),
    )?;
    immutable_value(
        tx,
        &immutable,
        parts,
        charge,
        total(live, [reference.len(), immutable.len()])?,
    )?;
    write_alias(tx, scope, &reference, alias, charge, live)
}

fn write_alias(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    reference: &[u8],
    alias: ValueAlias<'_>,
    charge: &mut ValueCharge,
    live: usize,
) -> Result<(), Error> {
    let alias_key = tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            reference,
        ],
        0,
        |maximum| key::encode_alias_key(scope, reference, maximum),
    )?;
    let alias_len = total(119, [alias.partition.len(), alias.id.len()])?;
    charge.work(product(alias_len, 2)?)?;
    let encoded = value::encode_alias(alias, alias_len).map_err(admission::value_error)?;
    write_exact(tx, &alias_key, &encoded, charge, live)
}

fn publish_binding(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    step: &ValueStep<'_>,
    current: bool,
    charge: &mut ValueCharge,
    live: usize,
) -> Result<(), Error> {
    publish_binding_fields(tx, scope, step.plan, step.binding, current, charge, live)
}

pub(super) fn publish_binding_fields(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    scoped: &wf_domain::workflow::batch::ScopedPlan,
    binding: value::ValueBindingData,
    current: bool,
    charge: &mut ValueCharge,
    live: usize,
) -> Result<(), Error> {
    charge.helper()?;
    let record = &scoped.plan().write().ok_or(Error::InvalidChanges)?.record;
    let event = scoped.plan().history().ok_or(Error::InvalidChanges)?;
    let history = (!current).then_some(event.event_id.as_str());
    let logical = total(
        scope_bytes(scope),
        [
            scoped.partition().as_bytes().len(),
            record.id().as_bytes().len(),
            history.map_or(0, str::len),
        ],
    )?;
    let bound = total(64, [product(logical, 2)?])?;
    charge.work(total(128, [product(logical, 16)?, 394])?)?;
    charge.temporary(total(live, [bound, 197])?)?;
    let key = tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            scoped.partition().as_bytes(),
            record.id().as_bytes(),
            history.unwrap_or("").as_bytes(),
        ],
        8,
        |maximum| key::encode_binding_key(scope, scoped.partition(), record.id(), history, maximum),
    )?;
    let encoded = value::encode_binding(binding, 197).map_err(admission::value_error)?;
    if current {
        put(tx, &key, &encoded, charge, live)
    } else {
        write_exact(tx, &key, &encoded, charge, live)
    }
}
