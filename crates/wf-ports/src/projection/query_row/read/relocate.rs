//! Typed state-location CAS and canonical source row re-emission.
use super::super::{
    I,
    budget::{Budget, add},
    metadata,
    model::OriginalQueryRecordError as E,
    record::{Fields, RecordPlan},
    writer::Writer,
};
use super::model::{
    DecodedOriginalQueryRow, OriginalQueryDecodePlan as Plan, OriginalQueryLocation as Location,
    OriginalQueryScope, OriginalQuerySelection as A, OriginalQueryStateSelection as S,
};
pub(super) fn run(
    encoded: &[u8],
    key: &[u8],
    expected: &Location<'_>,
    relocated: &Location<'_>,
    b: &mut Budget,
) -> Result<Vec<u8>, E> {
    if !same_logical(expected, relocated, b)? {
        return Err(E::LogicalGenerationMismatch);
    }
    let row = super::parse(
        encoded,
        key,
        Some(Plan {
            attributes: A::None,
            state_meta: S::None,
        }),
        false,
        None,
        b,
    )?;
    if !same_physical(row.locator(), expected, b)? {
        return Err(E::LocatorCompareFailed);
    }
    let location = super::locator::plan(relocated, row.expiry, b)?;
    if relocated.flow_id != row.id || relocated.version != row.version {
        return Err(E::InvalidRecord);
    }
    let record = row.record.as_ref().ok_or(E::InvalidRecord)?;
    let fields = Fields::from_scalar_values(&record.fields);
    let partition = match record.field("partition_key") {
        Some(I::Nil) => None,
        Some(I::Binary(bytes)) => Some(bytes),
        _ => return Err(E::InvalidRecord),
    };
    b.spend(row.id.len())?;
    let root_is_id = matches!(record.field("root_flow_id"),Some(I::Binary(bytes)) if bytes==row.id);
    let plan = RecordPlan {
        fields,
        id: row.id,
        version: row.version,
        partition,
        scope: None,
        root_is_id,
        sections: [0; 5],
        metadata_bytes: 0,
    };
    let mut measured = Writer::measure();
    metadata::builtin(&plan, &mut measured, b)?;
    let mut sizes = [0; 5];
    sizes[0] = measured.length;
    for (slot, bytes) in sizes.iter_mut().zip(row.sections).skip(1) {
        *slot = bytes.len();
    }
    if sizes[3] > 4096 || sizes[4] > 20480 {
        return Err(E::InvalidRecord);
    }
    let metadata_len = sizes.iter().try_fold(11, |n, size| add(n, *size))?;
    let mut measured = Writer::measure();
    header(&row, location.length, metadata_len, &mut measured, b)?;
    let length = add(add(measured.length, location.length)?, metadata_len)?;
    if length > 32768 {
        return Err(E::InvalidRecord);
    }
    let mut out = Writer::owned(length, b)?;
    header(&row, location.length, metadata_len, &mut out, b)?;
    super::super::locator::emit(&location, &mut out, b)?;
    out.byte(u8::from(root_is_id), b)?;
    for size in sizes {
        out.u16(size, b)?;
    }
    metadata::builtin(&plan, &mut out, b)?;
    for bytes in row.sections.into_iter().skip(1) {
        out.put(bytes, b)?;
    }
    let mut result = out.finish()?;
    let body = result.get(9..).ok_or(E::Limit)?;
    b.spend(add(key.len(), body.len())?.checked_mul(8).ok_or(E::Limit)?)?;
    let crc = super::super::row::checksum(key, body);
    result
        .get_mut(5..9)
        .ok_or(E::Limit)?
        .copy_from_slice(&crc.to_be_bytes());
    Ok(result)
}
fn header(
    row: &DecodedOriginalQueryRow<'_>,
    locator: usize,
    metadata: usize,
    out: &mut Writer,
    b: &mut Budget,
) -> Result<(), E> {
    out.put(b"\xf7FQR\x01\0\0\0\0", b)?;
    out.varint(row.version, b)?;
    out.varint(row.expiry, b)?;
    let scope = match row.scope {
        OriginalQueryScope::NotRead => return Err(E::InvalidRecord),
        OriginalQueryScope::Unscoped => &[][..],
        OriginalQueryScope::Scoped(bytes) => bytes,
    };
    out.u16(scope.len(), b)?;
    out.u16(locator, b)?;
    out.u16(metadata, b)?;
    out.put(scope, b)
}
fn same_logical(a: &Location<'_>, c: &Location<'_>, b: &mut Budget) -> Result<bool, E> {
    b.spend(add(a.flow_id.len(), c.flow_id.len())?)?;
    Ok(a.flow_id == c.flow_id && a.version == c.version && a.raft_index == c.raft_index)
}
fn same_physical(a: &Location<'_>, c: &Location<'_>, b: &mut Budget) -> Result<bool, E> {
    let logical = same_logical(a, c, b)?;
    b.spend(add(a.checksum.len(), c.checksum.len())?)?;
    Ok(logical
        && a.source_tag == c.source_tag
        && a.source_id == c.source_id
        && a.offset == c.offset
        && a.value_size == c.value_size
        && a.frame_size == c.frame_size
        && a.checksum == c.checksum
        && a.segment_generation == c.segment_generation)
}
