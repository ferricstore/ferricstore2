//! One committed snapshot, one server clock pair and independently owned bytes.

use redb::{ReadableDatabase, TableHandle};
use sha2::{Digest, Sha256};
use std::mem::size_of;
use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::{
    OriginalValuePartition, OriginalValueRef, OriginalValueRefKind, WorkflowValueReadBatch,
    WorkflowValueReadFailure, WorkflowValueReadLimits, parse_original_value_ref,
    render_original_value_ref,
};
use wf_ports::clock::WorkflowValueReadClock;
use wf_ports::state::StateTransactionError as Error;
use wf_types::version::{HlcTimestamp, TimestampMillis};

use super::super::{META, ROWS, StateStoreLimits, admission, store::RedbStateStore};
use super::{
    budget::{ValueCharge, product, total},
    manifest,
};
use crate::state::{key, value};

mod sized;
pub(in crate::state::redb) use sized::read_captured;
pub(super) use sized::read_sized;

pub(crate) struct ValueReadContext {
    read: redb::ReadTransaction,
    pub(super) rows: redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    metadata: redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    pub(super) charge: ValueCharge,
    pub(super) limits: StateStoreLimits,
    pub(super) alias_format: u16,
    pub(super) defaults: wf_ports::consensus::WorkflowValueDefaults,
}

pub(super) fn fixed_owner() -> Result<usize, Error> {
    total(
        size_of::<ValueReadContext>(),
        [META.name().len(), ROWS.name().len()],
    )
}

pub(super) fn open(
    store: &RedbStateStore,
    mut charge: ValueCharge,
) -> Result<ValueReadContext, Error> {
    store.ready()?;
    charge.helper()?;
    charge.temporary(fixed_owner()?)?;
    let read = store
        .database
        .begin_read()
        .map_err(admission::transaction_error)?;
    let rows = read.open_table(ROWS).map_err(admission::table_error)?;
    let metadata = read.open_table(META).map_err(admission::table_error)?;
    let raw = metadata
        .get(b"manifest".as_slice())
        .map_err(|e| admission::storage_error(&e))?
        .ok_or(Error::Corrupt)?;
    charge.work(total(128, [raw.value().len()])?)?;
    let facts = manifest::parse(raw.value(), &store.group, &store.recovery)?;
    drop(raw);
    Ok(ValueReadContext {
        read,
        rows,
        metadata,
        charge,
        limits: store.limits,
        alias_format: facts.alias_format,
        defaults: facts.defaults,
    })
}

pub(super) fn read<C: WorkflowValueReadClock>(
    store: &RedbStateStore,
    clock: &mut C,
    scope: &RequestScope,
    references: &[&[u8]],
    limits: WorkflowValueReadLimits,
) -> Result<WorkflowValueReadBatch, Error> {
    store.ready()?;
    check_scope(store, scope)?;
    if references.len() > limits.maximum_references {
        return Err(Error::Limit);
    }
    let mut charge = ValueCharge::new(
        limits.maximum_references.max(1),
        limits.maximum_stored_bytes,
        limits.maximum_work,
        limits.maximum_temporary_bytes,
    )?;
    let fixed = total(size_of::<WorkflowValueReadBatch>(), [fixed_owner()?])?;
    let slot_bytes = product(
        references.len(),
        size_of::<Result<Option<usize>, WorkflowValueReadFailure>>(),
    )?;
    let mut eligible_count = 0_usize;
    charge.work(product(references.len(), 128)?)?;
    for reference in references {
        charge.work(total(128, [product(reference.len(), 4)?])?)?;
        eligible_count += usize::from(eligible(reference));
    }
    let mut slots = charge.reserve(references.len(), fixed)?;
    if eligible_count == 0 {
        slots.resize(references.len(), Ok(None));
        return Ok(WorkflowValueReadBatch {
            values: Vec::new(),
            slots,
        });
    }
    if !matches!(store.layout.get(), 7 | 8) {
        return Err(Error::UnsupportedLayout);
    }
    let mut context = open(store, charge).map_err(|e| store.failed(e))?;
    // A clock failure is not an engine failure and must not fence the database.
    let time = clock.read_snapshot()?;
    let mut values = context
        .charge
        .reserve(references.len(), total(fixed, [slot_bytes])?)?;
    let fixed = total(
        fixed,
        [slot_bytes, product(references.len(), size_of::<Vec<u8>>())?],
    )?;
    let mut returned = 0_usize;
    context.charge.work(product(references.len(), 128)?)?;
    for (index, reference) in references.iter().enumerate() {
        context
            .charge
            .work(total(128, [product(reference.len(), 4)?])?)?;
        if !eligible(reference) {
            slots.push(Ok(None));
            continue;
        }
        let mut duplicate = None;
        context.charge.work(product(index, 128)?)?;
        for (previous, candidate) in references.iter().enumerate().take(index) {
            context.charge.compare(reference.len(), candidate.len())?;
            if reference == candidate {
                duplicate = Some(previous);
                break;
            }
        }
        if let Some(previous) = duplicate {
            slots.push(slots[previous]);
            continue;
        }
        let next = one(
            &mut context,
            scope,
            reference,
            time,
            total(fixed, [returned])?,
        )
        .map_err(|e| store.failed(e))?;
        match next {
            Ok(Some(bytes)) => {
                returned = total(returned, [bytes.len()])?;
                let position = values.len();
                values.push(bytes);
                slots.push(Ok(Some(position)));
            }
            Ok(None) => slots.push(Ok(None)),
            Err(error) => slots.push(Err(error)),
        }
    }
    close(context);
    Ok(WorkflowValueReadBatch { values, slots })
}

pub(super) fn eligible(reference: &[u8]) -> bool {
    !reference.is_empty()
        && reference.len() <= 65_535
        && parse_original_value_ref(reference).is_some()
}

fn one(
    context: &mut ValueReadContext,
    scope: &RequestScope,
    reference: &[u8],
    time: (HlcTimestamp, TimestampMillis),
    live: usize,
) -> Result<Result<Option<Vec<u8>>, WorkflowValueReadFailure>, Error> {
    context.charge.helper()?;
    if !matches!(context.alias_format, 1 | 2) {
        return Err(Error::UnsupportedLayout);
    }
    context.charge.work(product(reference.len(), 4)?)?;
    let Some(OriginalValueRef::Regular {
        kind:
            OriginalValueRefKind::Payload | OriginalValueRefKind::Result | OriginalValueRefKind::Error,
        version,
        ..
    }) = parse_original_value_ref(reference)
    else {
        return Ok(Err(WorkflowValueReadFailure::UnsupportedProfile));
    };
    if version == 0 {
        return Ok(Ok(None));
    }
    context.charge.work(total(
        128,
        [product(total(reference.len(), [scope_bytes(scope)])?, 16)?],
    )?)?;
    let bound = total(
        64,
        [product(total(reference.len(), [scope_bytes(scope)])?, 2)?],
    )?;
    context.charge.temporary(total(live, [bound])?)?;
    let alias_key = key::encode_alias_key(
        scope,
        reference,
        bound.min(context.limits.maximum_temporary_bytes()),
    )
    .map_err(admission::value_error)?;
    context.charge.candidate()?;
    let Some(raw) = context
        .rows
        .get(alias_key.as_slice())
        .map_err(|e| admission::storage_error(&e))?
    else {
        return Ok(Ok(None));
    };
    context.charge.work(total(128, [raw.value().len()])?)?;
    let alias = checked_alias(
        raw.value(),
        reference,
        &mut context.charge,
        total(live, [alias_key.len()])?,
        context.alias_format,
    )?;
    immutable_value(
        &context.rows,
        &mut context.charge,
        scope,
        reference,
        alias,
        time,
        (live, bound, alias_key.len(), raw.value().len()),
    )
}

fn immutable_value(
    rows: &redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    charge: &mut ValueCharge,
    scope: &RequestScope,
    reference: &[u8],
    alias: value::ValueAlias<'_>,
    time: (HlcTimestamp, TimestampMillis),
    (live, bound, alias_key_bytes, alias_bytes): (usize, usize, usize, usize),
) -> Result<Result<Option<Vec<u8>>, WorkflowValueReadFailure>, Error> {
    let size = usize::try_from(alias.stored_len).map_err(|_| Error::Limit)?;
    charge.work(total(
        128,
        [product(total(reference.len(), [scope_bytes(scope)])?, 16)?],
    )?)?;
    let immutable_bound = total(bound, [16])?;
    charge.temporary(total(
        live,
        [alias_key_bytes, alias_bytes, immutable_bound],
    )?)?;
    let immutable = key::encode_immutable_key(scope, reference, alias.incarnation, immutable_bound)
        .map_err(admission::value_error)?;
    charge.candidate()?;
    let bytes = rows
        .get(immutable.as_slice())
        .map_err(|e| admission::storage_error(&e))?
        .ok_or(Error::Corrupt)?;
    charge.work(total(128, [size])?)?;
    authenticate(bytes.value(), size, &alias.digest)?;
    match expiry(alias.expires_at, time.0, time.1) {
        Err(error) => return Ok(Err(error)),
        Ok(false) => return Ok(Ok(None)),
        Ok(true) => {}
    }
    let live = total(live, [bound, immutable.len(), alias_bytes])?;
    owned_value(charge, bytes.value(), live).map(|bytes| Ok(Some(bytes)))
}

fn authenticate(bytes: &[u8], size: usize, digest: &[u8; 32]) -> Result<(), Error> {
    if bytes.len() != size || Sha256::digest(bytes).as_slice() != digest {
        return Err(Error::Corrupt);
    }
    Ok(())
}

fn owned_value(charge: &mut ValueCharge, bytes: &[u8], live: usize) -> Result<Vec<u8>, Error> {
    charge.helper()?;
    charge.stored(bytes.len())?;
    charge.work(bytes.len())?;
    charge.temporary(total(live, [bytes.len()])?)?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(bytes.len())
        .map_err(|_| Error::Allocation)?;
    result.extend_from_slice(bytes);
    Ok(result)
}

fn checked_alias<'encoded>(
    encoded: &'encoded [u8],
    reference: &[u8],
    charge: &mut ValueCharge,
    live: usize,
    alias_format: u16,
) -> Result<value::ValueAlias<'encoded>, Error> {
    charge.helper()?;
    charge.temporary(total(live, [65_535, encoded.len()])?)?;
    let alias = value::decode_alias(encoded).map_err(admission::value_error)?;
    if alias.stored && alias_format != 2 {
        return Err(Error::Corrupt);
    }
    charge.work(total(
        128,
        [
            product(total(alias.partition.len(), [alias.id.len()])?, 4)?,
            reference.len(),
        ],
    )?)?;
    let rendered = render_original_value_ref(
        alias.id,
        alias.kind,
        alias.version,
        OriginalValuePartition::Explicit(alias.partition),
        65_535,
    )
    .map_err(|error| build_error(&error))?;
    charge.compare(rendered.len(), reference.len())?;
    if rendered != reference {
        return Err(Error::Corrupt);
    }
    Ok(alias)
}

pub(super) fn expiry(
    expires: Option<TimestampMillis>,
    hlc: HlcTimestamp,
    wall: TimestampMillis,
) -> Result<bool, WorkflowValueReadFailure> {
    let Some(expires) = expires.filter(|time| time.get() != 0) else {
        return Ok(true);
    };
    if expires > hlc.physical() {
        return Ok(true);
    }
    if hlc
        .physical()
        .get()
        .checked_sub(wall.get())
        .is_some_and(|drift| drift >= 1_000)
        && expires > wall
    {
        return Err(WorkflowValueReadFailure::UnsafeDrift);
    }
    Ok(false)
}

pub(super) fn check_scope(store: &RedbStateStore, scope: &RequestScope) -> Result<(), Error> {
    if scope.group() != &store.group || scope.recovery() != &store.recovery {
        return Err(Error::InvalidChanges);
    }
    Ok(())
}

pub(crate) fn scope_bytes(scope: &RequestScope) -> usize {
    scope.tenant().as_bytes().len()
        + scope.group().as_bytes().len()
        + scope.recovery().as_bytes().len()
}

pub(crate) fn build_error(error: &wf_ports::blob::OriginalValueBuildError) -> Error {
    match error {
        wf_ports::blob::OriginalValueBuildError::ReserveFailed(_) => Error::Allocation,
        wf_ports::blob::OriginalValueBuildError::Limit => Error::Limit,
        wf_ports::blob::OriginalValueBuildError::InvalidInput => Error::InvalidChanges,
    }
}

fn close(context: ValueReadContext) {
    drop(context.rows);
    drop(context.metadata);
    drop(context.read);
}
