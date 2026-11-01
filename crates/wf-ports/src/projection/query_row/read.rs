//! Original row readers and typed state-location relocation, without authority.
mod model;
use super::model::{OriginalQueryRecordError as E, PreparedOriginalQueryRow};
pub use model::*;

mod cursor;
mod dynamic;
mod frame;
mod internal;
mod locator;
mod metadata;
mod relocate;
mod selection;
use super::budget::Budget;

/// Decode all original metadata, then optionally apply expiry.
/// # Errors
/// Invalid source bytes and independent operational refusals remain distinct.
pub fn decode_original_query_row<'a>(
    encoded: &'a [u8],
    key: &'a [u8],
    now: Option<u64>,
    limits: OriginalQueryReadLimits,
) -> Result<OriginalQueryRead<DecodedOriginalQueryRow<'a>>, E> {
    let mut b = frame::budget(limits, encoded, key)?;
    let row = parse(encoded, key, None, false, now, &mut b)?;
    Ok(finish(row, now, &b))
}

/// Decode selected public fields while validating every skipped metadata field.
/// # Errors
/// Refuses invalid source rows/plans and independent resource ceilings.
pub fn decode_original_query_for_query<'a>(
    encoded: &'a [u8],
    key: &'a [u8],
    now: u64,
    plan: OriginalQueryDecodePlan<'_>,
    limits: OriginalQueryReadLimits,
) -> Result<OriginalQueryRead<DecodedOriginalQueryRow<'a>>, E> {
    let mut b = frame::budget(limits, encoded, key)?;
    selection::validate(plan, &mut b)?;
    let row = parse(encoded, key, Some(plan), false, Some(now), &mut b)?;
    Ok(finish(row, Some(now), &b))
}

/// Decode a hydration reference without interpreting scope or record metadata.
/// # Errors
/// Refuses invalid framing/checksum/locator and operational ceilings.
pub fn decode_original_query_reference<'a>(
    encoded: &'a [u8],
    key: &'a [u8],
    now: u64,
    limits: OriginalQueryReadLimits,
) -> Result<OriginalQueryRead<DecodedOriginalQueryRow<'a>>, E> {
    let mut b = frame::budget(limits, encoded, key)?;
    let row = parse(encoded, key, None, true, Some(now), &mut b)?;
    Ok(finish(row, Some(now), &b))
}

/// Relocate typed state-location data, preserving original comparison order.
/// This profile uses u64 numeric/state-kind data, not arbitrary VM struct equality.
/// # Errors
/// Distinguishes generation/CAS/source-invalid and operational errors.
pub fn relocate_original_query_row(
    encoded: &[u8],
    key: &[u8],
    expected: &OriginalQueryLocation<'_>,
    relocated: &OriginalQueryLocation<'_>,
    limits: OriginalQueryReadLimits,
) -> Result<PreparedOriginalQueryRow, E> {
    let mut b = frame::budget(limits, encoded, key)?;
    let encoded = relocate::run(encoded, key, expected, relocated, &mut b)?;
    Ok(PreparedOriginalQueryRow {
        encoded,
        work: b.work,
        temporary: b.peak,
    })
}

/// Restore original internal partition and terminal-retention overrides.
/// # Errors
/// Reference-only rows and operational refusals remain errors.
pub fn prepare_original_query_internal_record<'r, 'a>(
    row: &'r DecodedOriginalQueryRow<'a>,
    limits: OriginalQueryReadLimits,
) -> Result<OriginalQueryRead<OriginalQueryInternalRecord<'r, 'a>>, E> {
    let mut b = frame::budget(limits, row.encoded, row.key)?;
    let value = internal::run(row, &mut b)?;
    Ok(OriginalQueryRead::Row {
        value,
        work_units: b.work,
        temporary_bytes: b.peak,
    })
}

fn parse<'a>(
    encoded: &'a [u8],
    key: &'a [u8],
    plan: Option<OriginalQueryDecodePlan<'_>>,
    reference: bool,
    now: Option<u64>,
    b: &mut Budget,
) -> Result<DecodedOriginalQueryRow<'a>, E> {
    let f = frame::parse(encoded, key, b)?;
    let location = locator::parse(&f, b)?;
    let plan = if now.is_some_and(|now| f.expiry > 0 && f.expiry <= now) {
        Some(OriginalQueryDecodePlan {
            attributes: OriginalQuerySelection::None,
            state_meta: OriginalQueryStateSelection::None,
        })
    } else {
        plan
    };
    let (record, scope, sections) = if reference {
        (None, OriginalQueryScope::NotRead, [&[][..]; 5])
    } else {
        let (record, sections) = metadata::parse(&f, plan, b)?;
        (
            Some(record),
            f.scope
                .map_or(OriginalQueryScope::Unscoped, OriginalQueryScope::Scoped),
            sections,
        )
    };
    Ok(DecodedOriginalQueryRow {
        encoded,
        key,
        id: f.id,
        version: f.version,
        expiry: f.expiry,
        location,
        scope,
        record,
        sections,
    })
}
fn finish<'a>(
    row: DecodedOriginalQueryRow<'a>,
    now: Option<u64>,
    b: &Budget,
) -> OriginalQueryRead<DecodedOriginalQueryRow<'a>> {
    if now.is_some_and(|now| row.expiry > 0 && row.expiry <= now) {
        OriginalQueryRead::Expired {
            work_units: b.work,
            temporary_bytes: b.peak,
        }
    } else {
        OriginalQueryRead::Row {
            value: row,
            work_units: b.work,
            temporary_bytes: b.peak,
        }
    }
}
