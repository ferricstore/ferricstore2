//! Bounded private physical-query grammar; fresh semantics belong to domain apply.

use crate::consensus::{PreparedClaimPhase, PreparedClaimQuery};

use super::{CodecError, CodecLimits, reader::Reader, writer::Writer};

const MAX_FILTER_ITEMS: usize = 100_000;

mod apply;
mod response;

pub use apply::{
    ClaimApplyDecision, ClaimApplyPlan, ClaimPhaseInspection, FreshClaimPhase,
    decide_fresh_claim_phase_observed, inspect_claim_phase_point, prepare_claim_phase,
};
pub use response::{claim_response_matches, claim_retained_summary};

pub(super) type ClaimFilterRefs<'a> = [Vec<&'a [u8]>; 3];
type FilterParts<'a> = (Option<Vec<Vec<u8>>>, Vec<&'a [u8]>);
type ComponentParts<'a> = (Vec<Vec<u8>>, Vec<&'a [u8]>);

pub(super) fn write(
    phase: &PreparedClaimPhase,
    writer: &mut Writer<'_>,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    let query = &phase.query;
    writer.raw(&query.workflow_type, limits.admission().inline_bytes())?;
    write_filter(query.states.as_deref(), writer, limits)?;
    write_components(&query.exclude_states, writer, limits)?;
    write_filter(query.partitions.as_deref(), writer, limits)?;
    writer.optional(query.priority.as_ref(), |writer, value| writer.byte(*value))?;
    writer.raw(&query.worker, limits.admission().inline_bytes())?;
    writer.u64(query.lease_ms)?;
    for value in [query.limit, phase.scan_budget, phase.head_budget] {
        writer.u32(u32::try_from(value).map_err(|_| CodecError::LengthOverflow)?)?;
    }
    writer.u16(phase.token_version)?;
    writer.u32(phase.first_ordinal)
}

pub(super) fn preflight(reader: &mut Reader<'_>) -> Result<(), CodecError> {
    occurrences(reader).map(|_| ())
}

pub(super) fn occurrences(reader: &mut Reader<'_>) -> Result<usize, CodecError> {
    reader.field()?;
    let states = scan_filter(reader)?;
    let exclusions = scan_components(reader)?;
    let partitions = scan_filter(reader)?;
    reader.optional(Reader::byte)?;
    reader.field()?;
    reader.u64()?;
    for _ in 0..3 {
        usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)?;
    }
    reader.u16()?;
    reader.u32()?;
    states
        .checked_add(exclusions)
        .and_then(|count| count.checked_add(partitions))
        .ok_or(CodecError::LengthOverflow)
}

pub(super) fn read<'a>(
    reader: &mut Reader<'a>,
) -> Result<(PreparedClaimPhase, ClaimFilterRefs<'a>), CodecError> {
    let workflow_type = reader.owned()?;
    let (states, state_refs) = read_filter(reader)?;
    let (exclude_states, excluded_refs) = read_components(reader)?;
    let (partitions, partition_refs) = read_filter(reader)?;
    let priority = reader.optional(Reader::byte)?;
    let worker = reader.owned()?;
    let lease_ms = reader.u64()?;
    let limit = usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)?;
    let scan_budget = usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)?;
    let head_budget = usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)?;
    let token_version = reader.u16()?;
    let first_ordinal = reader.u32()?;
    Ok((
        PreparedClaimPhase {
            query: PreparedClaimQuery {
                workflow_type,
                states,
                exclude_states,
                partitions,
                priority,
                worker,
                lease_ms,
                limit,
            },
            token_version,
            first_ordinal,
            scan_budget,
            head_budget,
        },
        [state_refs, excluded_refs, partition_refs],
    ))
}

fn write_filter(
    values: Option<&[Vec<u8>]>,
    writer: &mut Writer<'_>,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    writer.optional(values, |writer, values| {
        write_components(values, writer, limits)
    })
}

fn write_components(
    values: &[Vec<u8>],
    writer: &mut Writer<'_>,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    if values.len() > MAX_FILTER_ITEMS {
        return Err(CodecError::ItemLimit);
    }
    writer.u32(u32::try_from(values.len()).map_err(|_| CodecError::LengthOverflow)?)?;
    for value in values {
        writer.raw(value, limits.admission().inline_bytes())?;
    }
    Ok(())
}

fn scan_filter(reader: &mut Reader<'_>) -> Result<usize, CodecError> {
    reader
        .optional(scan_components)
        .map(Option::unwrap_or_default)
}

fn scan_components(reader: &mut Reader<'_>) -> Result<usize, CodecError> {
    let count = usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)?;
    if count > MAX_FILTER_ITEMS {
        return Err(CodecError::ItemLimit);
    }
    if count > reader.remaining() / 4 {
        return Err(CodecError::Malformed);
    }
    for _ in 0..count {
        reader.field()?;
    }
    Ok(count)
}

fn read_filter<'a>(reader: &mut Reader<'a>) -> Result<FilterParts<'a>, CodecError> {
    match reader.byte()? {
        0 => Ok((None, Vec::new())),
        1 => {
            let (values, references) = read_components(reader)?;
            Ok((Some(values), references))
        }
        _ => Err(CodecError::InvalidOptionality),
    }
}

fn read_components<'a>(reader: &mut Reader<'a>) -> Result<ComponentParts<'a>, CodecError> {
    let count = usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)?;
    if count > MAX_FILTER_ITEMS {
        return Err(CodecError::ItemLimit);
    }
    if count > reader.remaining() / 4 {
        return Err(CodecError::Malformed);
    }
    let mut values = Vec::new();
    let mut references = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| CodecError::Allocation)?;
    references
        .try_reserve_exact(count)
        .map_err(|_| CodecError::Allocation)?;
    for _ in 0..count {
        let raw = reader.field()?;
        values.push(super::reader::copy(raw)?);
        references.push(raw);
    }
    Ok((values, references))
}
