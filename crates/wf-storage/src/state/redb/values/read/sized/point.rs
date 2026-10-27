//! Shared sized-point authentication, expiry, omission and owned copy.

use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::{
    OriginalValueRef, OriginalValueRefKind, WorkflowValueReadFailure, WorkflowValueSizedReadValue,
    parse_original_value_ref,
};
use wf_ports::state::StateTransactionError as Error;
use wf_types::version::{HlcTimestamp, TimestampMillis};

use super::super::super::budget::{ValueCharge, product, total};
use super::super::{self as read, StateStoreLimits, ValueReadContext, admission, scope_bytes};
use crate::state::{key, value};

pub(super) fn one(
    context: &mut ValueReadContext,
    scope: &RequestScope,
    reference: &[u8],
    time: (HlcTimestamp, TimestampMillis),
    threshold: usize,
    live: usize,
) -> Result<Result<Option<WorkflowValueSizedReadValue>, WorkflowValueReadFailure>, Error> {
    captured(
        &context.rows,
        &mut context.charge,
        (context.limits, context.alias_format, None),
        scope,
        reference,
        time,
        (threshold, live),
    )
}

pub(super) fn captured(
    rows: &redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    charge: &mut ValueCharge,
    (limits, alias_format, expected_incarnation): (StateStoreLimits, u16, Option<u128>),
    scope: &RequestScope,
    reference: &[u8],
    time: (HlcTimestamp, TimestampMillis),
    (threshold, live): (usize, usize),
) -> Result<Result<Option<WorkflowValueSizedReadValue>, WorkflowValueReadFailure>, Error> {
    charge.helper()?;
    charge.work(product(reference.len(), 4)?)?;
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
    charge.work(total(
        128,
        [product(total(reference.len(), [scope_bytes(scope)])?, 16)?],
    )?)?;
    let bound = total(
        64,
        [product(total(reference.len(), [scope_bytes(scope)])?, 2)?],
    )?;
    charge.temporary(total(live, [bound])?)?;
    let alias_key = key::encode_alias_key(
        scope,
        reference,
        bound.min(limits.maximum_temporary_bytes()),
    )
    .map_err(admission::value_error)?;
    charge.candidate()?;
    let Some(raw) = rows
        .get(alias_key.as_slice())
        .map_err(|e| admission::storage_error(&e))?
    else {
        return Ok(Ok(None));
    };
    charge.work(total(128, [raw.value().len()])?)?;
    charge.temporary(total(live, [alias_key.len(), raw.value().len()])?)?;
    let alias = read::checked_alias(
        raw.value(),
        reference,
        charge,
        total(live, [alias_key.len()])?,
        alias_format,
    )?;
    if let Some(expected) = expected_incarnation {
        charge.candidate()?;
        if alias.incarnation != expected {
            return Err(Error::Corrupt);
        }
    }
    immutable(
        rows,
        charge,
        scope,
        reference,
        (alias, alias_key.len(), raw.value().len(), bound),
        time,
        (threshold, live),
    )
}

fn immutable(
    rows: &redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    charge: &mut ValueCharge,
    scope: &RequestScope,
    reference: &[u8],
    (alias, alias_key_bytes, alias_bytes, bound): (value::ValueAlias<'_>, usize, usize, usize),
    time: (HlcTimestamp, TimestampMillis),
    (threshold, live): (usize, usize),
) -> Result<Result<Option<WorkflowValueSizedReadValue>, WorkflowValueReadFailure>, Error> {
    let size = usize::try_from(alias.stored_len).map_err(|_| Error::Limit)?;
    charge.work(total(
        128,
        [product(total(reference.len(), [scope_bytes(scope)])?, 16)?],
    )?)?;
    let immutable_bound = total(bound, [16])?;
    let guard_live = total(live, [alias_key_bytes, alias_bytes, immutable_bound, size])?;
    charge.temporary(guard_live)?;
    let immutable = key::encode_immutable_key(scope, reference, alias.incarnation, immutable_bound)
        .map_err(admission::value_error)?;
    // Fund the complete guard extent and hash before acquiring the immutable row.
    charge.work(total(128, [size])?)?;
    charge.candidate()?;
    let bytes = rows
        .get(immutable.as_slice())
        .map_err(|e| admission::storage_error(&e))?
        .ok_or(Error::Corrupt)?;
    // The shared predicate checks actual length before traversing any bytes.
    read::authenticate(bytes.value(), size, &alias.digest)?;
    charge.candidate()?;
    match read::expiry(alias.expires_at, time.0, time.1) {
        Err(error) => return Ok(Err(error)),
        Ok(false) => return Ok(Ok(None)),
        Ok(true) => {}
    }
    charge.candidate()?;
    if size >= threshold {
        return Ok(Ok(Some(WorkflowValueSizedReadValue::Omitted {
            stored_bytes: size,
        })));
    }
    // The current guard extent remains live while the independent copy is funded.
    read::owned_value(charge, bytes.value(), guard_live)
        .map(|bytes| Ok(Some(WorkflowValueSizedReadValue::Stored(bytes))))
}
