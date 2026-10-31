//! One payment grammar, complete borrowed scanning and bounded ID uniqueness.
use super::{E, GovernedClaimPayment as P, GovernedClaimPaymentRef as R};
use crate::log::{reader::Reader, writer::Writer};
use wf_domain::{governance::release::LimitEnforcement as F, workflow::MAX_EXACT_MILLIS as MAX};

/// Encodes complete original payment data; it proves no spend or live ownership.
/// # Errors
/// Refuses malformed data, duplicate original IDs or prospective byte/owner limits.
pub fn encode_claim_payment(
    payment: P<'_>,
    limits: crate::log::CodecLimits,
    maximum_owned: usize,
) -> Result<Vec<u8>, E> {
    let mut measured = Writer::measure(limits.admission().decision_bytes());
    write(&mut measured, payment)?;
    let required = measured.len().checked_add(4096).ok_or(E::LengthOverflow)?;
    let scratch = payment
        .reservation_ids
        .len()
        .checked_mul(size_of::<&[u8]>())
        .and_then(|n| n.checked_add(256))
        .ok_or(E::LengthOverflow)?;
    if required.checked_add(scratch).ok_or(E::LengthOverflow)? > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut out = Writer::allocate(measured.len(), limits.admission().decision_bytes())?;
    write(&mut out, payment)?;
    let encoded = out.finish()?;
    inspect_claim_payment(&encoded, limits, scratch)?;
    Ok(encoded)
}

/// Completely scans payment data and EOF before bounded duplicate inspection.
/// # Errors
/// Refuses malformed/duplicate IDs, unbounded scope, trailing bytes or scratch limits.
pub fn inspect_claim_payment(
    encoded: &[u8],
    limits: crate::log::CodecLimits,
    maximum_scratch: usize,
) -> Result<R<'_>, E> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(E::ByteLimit);
    }
    let mut reader = Reader::new(encoded, limits);
    let payment = read(&mut reader, encoded)?;
    reader.finish()?;
    validate(payment, maximum_scratch)?;
    Ok(payment)
}

pub(in crate::log) fn write(out: &mut Writer<'_>, payment: P<'_>) -> Result<(), E> {
    metadata(
        payment.scope,
        payment.shard_id,
        payment.epoch,
        payment.reservation_ids.len(),
    )?;
    out.raw(payment.scope, 65535)?;
    out.u64(payment.shard_id)?;
    out.byte(match payment.enforcement {
        F::ApproximateGlobal => 0,
        F::StrictGlobal => 1,
    })?;
    out.u64(payment.epoch)?;
    out.u16(u16::try_from(payment.reservation_ids.len()).map_err(|_| E::ItemLimit)?)?;
    for id in payment.reservation_ids {
        if id.is_empty() {
            return Err(E::Malformed);
        }
        out.raw(id, 256)?;
    }
    Ok(())
}
pub(in crate::log) fn read<'a>(reader: &mut Reader<'a>, encoded: &'a [u8]) -> Result<R<'a>, E> {
    let start = encoded.len() - reader.remaining();
    let scope = reader.nonempty(65535)?;
    let shard = reader.u64()?;
    let enforcement = match reader.byte()? {
        0 => F::ApproximateGlobal,
        1 => F::StrictGlobal,
        _ => return Err(E::UnsupportedTag),
    };
    let epoch = reader.u64()?;
    let count = usize::from(reader.u16()?);
    metadata(scope, shard, epoch, count)?;
    let at = encoded.len() - reader.remaining();
    for _ in 0..count {
        reader.nonempty(256)?;
    }
    let end = encoded.len() - reader.remaining();
    Ok(R {
        body: &encoded[start..end],
        scope,
        shard,
        epoch,
        enforcement,
        count,
        ids: &encoded[at..end],
    })
}
fn metadata(scope: &[u8], shard: u64, epoch: u64, count: usize) -> Result<(), E> {
    if scope.is_empty()
        || scope.len() > 65535
        || shard > MAX
        || !(1..=MAX).contains(&epoch)
        || !(1..=1000).contains(&count)
    {
        return Err(E::Malformed);
    }
    Ok(())
}
pub(in crate::log) fn validate(payment: R<'_>, maximum_owned: usize) -> Result<(), E> {
    let bound = payment
        .count
        .checked_mul(std::mem::size_of::<&[u8]>())
        .and_then(|n| n.checked_add(256))
        .ok_or(E::LengthOverflow)?;
    if bound > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut ids = Vec::new();
    ids.try_reserve_exact(payment.count)
        .map_err(|_| E::Allocation)?;
    for id in payment.reservation_ids() {
        ids.push(id?);
    }
    ids.sort_unstable();
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(E::Malformed);
    }
    Ok(())
}
