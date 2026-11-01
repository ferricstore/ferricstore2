//! One final row owner, source key-bound CRC32 and bounded metadata sections.
use super::{
    I,
    budget::{Budget, add},
    locator, metadata,
    model::{OriginalQueryLocator, OriginalQueryRecordError as E},
    primitive,
    record::{self, RecordPlan},
    writer::Writer,
};

pub(super) fn encode(
    key: I<'_, '_>,
    input: I<'_, '_>,
    location: OriginalQueryLocator<'_, '_>,
    expiry: I<'_, '_>,
    budget: &mut Budget,
) -> Result<Vec<u8>, E> {
    let expiry = primitive::uint(expiry, budget)?;
    let plan = record::plan(key, input, budget)?;
    let location = locator::plan(location, &plan, expiry, budget)?;
    let mut measured = Writer::measure();
    header(&plan, location.length, expiry, &mut measured, budget)?;
    let length = add(add(measured.length, location.length)?, plan.metadata_bytes)?;
    if length > 32768 {
        return Err(E::InvalidRecord);
    }
    let mut out = Writer::owned(length, budget)?;
    header(&plan, location.length, expiry, &mut out, budget)?;
    locator::emit(&location, &mut out, budget)?;
    metadata::emit(&plan, &mut out, budget)?;
    let mut bytes = out.finish()?;
    let I::Binary(key) = key else {
        return Err(E::InvalidRecord);
    };
    let body = bytes.get(9..).ok_or(E::Limit)?;
    budget.spend(add(key.len(), body.len())?.checked_mul(8).ok_or(E::Limit)?)?;
    let checksum = checksum(key, body);
    bytes
        .get_mut(5..9)
        .ok_or(E::Limit)?
        .copy_from_slice(&checksum.to_be_bytes());
    Ok(bytes)
}
fn header(
    plan: &RecordPlan<'_, '_>,
    locator_length: usize,
    expiry: u64,
    out: &mut Writer,
    budget: &mut Budget,
) -> Result<(), E> {
    out.put(b"\xf7FQR\x01\0\0\0\0", budget)?;
    out.varint(plan.version, budget)?;
    out.varint(expiry, budget)?;
    let scope = plan.scope.as_deref().unwrap_or_default();
    out.u16(scope.len(), budget)?;
    out.u16(locator_length, budget)?;
    out.u16(plan.metadata_bytes, budget)?;
    out.put(scope, budget)
}
fn checksum(key: &[u8], body: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in key.iter().chain(body) {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 == 0 { 0 } else { 0xedb8_8320 };
        }
    }
    !crc
}
