//! Immutable values, aliases and every companion share the consumed core writer.

use redb::ReadableTable;
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
    if tx.layout != 7 {
        return Err(Error::UnsupportedLayout);
    }
    if !matches!(publication.mode, 1 | 5 | 6 | 7)
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
            _ => return Err(Error::InvalidChanges),
        };
        if outcome.intent().version() != 8 || outcome.response_version() != response {
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
                    (index, field.ok_or(Error::InvalidChanges)?),
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
    field: (usize, &[u8]),
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
    let stored_len =
        original_binary_stored_len(field.1, usize::MAX).map_err(|error| build_error(&error))?;
    let parts =
        original_binary_stored_parts(field.1, usize::MAX).map_err(|error| build_error(&error))?;
    charge.work(stored_len)?;
    let mut hash = Sha256::new();
    hash.update(parts[0]);
    hash.update(parts[1]);
    let alias = ValueAlias {
        stored: false,
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

fn immutable_value(
    tx: &RedbStateTransaction<'_>,
    key: &[u8],
    parts: [&[u8]; 2],
    charge: &mut ValueCharge,
    live: usize,
) -> Result<(), Error> {
    charge.candidate()?;
    tx.step()?;
    let table = tx
        .write
        .open_table(tx.rows_table)
        .map_err(admission::table_error)?;
    tx.step()?;
    let previous = table.get(key).map_err(|e| admission::storage_error(&e))?;
    let size = total(parts[0].len(), [parts[1].len()])?;
    if let Some(previous) = previous {
        charge.compare(previous.value().len(), size)?;
        if previous.value().len() != size
            || !previous.value().starts_with(parts[0])
            || &previous.value()[parts[0].len()..] != parts[1]
        {
            return Err(Error::Corrupt);
        }
        return Ok(());
    }
    drop(previous);
    drop(table);
    if parts[0].is_empty() {
        return put(tx, key, parts[1], charge, live);
    }
    charge.work(size)?;
    charge.temporary(total(live, [size])?)?;
    let mut stored = Vec::new();
    stored
        .try_reserve_exact(size)
        .map_err(|_| Error::Allocation)?;
    stored.extend_from_slice(parts[0]);
    stored.extend_from_slice(parts[1]);
    put(tx, key, &stored, charge, total(live, [stored.len()])?)
}

fn write_exact(
    tx: &RedbStateTransaction<'_>,
    key: &[u8],
    bytes: &[u8],
    charge: &mut ValueCharge,
    live: usize,
) -> Result<(), Error> {
    charge.candidate()?;
    tx.step()?;
    let table = tx
        .write
        .open_table(tx.rows_table)
        .map_err(admission::table_error)?;
    tx.step()?;
    if let Some(previous) = table.get(key).map_err(|e| admission::storage_error(&e))? {
        charge.compare(previous.value().len(), bytes.len())?;
        if previous.value() != bytes {
            return Err(Error::Corrupt);
        }
        return Ok(());
    }
    drop(table);
    put(tx, key, bytes, charge, live)
}

fn publish_binding(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    step: &ValueStep<'_>,
    current: bool,
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
    let event = step.plan.plan().history().ok_or(Error::InvalidChanges)?;
    let history = (!current).then_some(event.event_id.as_str());
    let logical = total(
        scope_bytes(scope),
        [
            step.plan.partition().as_bytes().len(),
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
            step.plan.partition().as_bytes(),
            record.id().as_bytes(),
            history.unwrap_or("").as_bytes(),
        ],
        8,
        |maximum| {
            key::encode_binding_key(scope, step.plan.partition(), record.id(), history, maximum)
        },
    )?;
    let encoded = value::encode_binding(step.binding, 197).map_err(admission::value_error)?;
    if current {
        put(tx, &key, &encoded, charge, live)
    } else {
        write_exact(tx, &key, &encoded, charge, live)
    }
}

fn put(
    tx: &RedbStateTransaction<'_>,
    key: &[u8],
    bytes: &[u8],
    charge: &mut ValueCharge,
    live: usize,
) -> Result<(), Error> {
    charge.output(1, total(key.len(), [bytes.len()])?)?;
    charge.work(total(key.len(), [bytes.len()])?)?;
    charge.temporary(total(live, [key.len(), bytes.len()])?)?;
    tx.temporary(total(key.len(), [bytes.len()])?)?;
    admission::engine_pair(key.len(), bytes.len())?;
    tx.step()?;
    let mut table = tx
        .write
        .open_table(tx.rows_table)
        .map_err(admission::table_error)?;
    tx.step()?;
    table
        .insert(key, bytes)
        .map_err(|e| admission::storage_error(&e))?;
    Ok(())
}

fn key_bound(scope: &RequestScope, reference: usize, scalar: usize) -> Result<usize, Error> {
    total(
        32,
        [product(total(scope_bytes(scope), [reference])?, 2)?, scalar],
    )
}
