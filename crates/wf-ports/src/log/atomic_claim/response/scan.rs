//! Complete whole-parent grammar; a rejected receipt contains no phase prefix.

use super::{
    AdmissionLimits, AtomicClaimReplyRef as Reply, CodecError as E, RetainedStatus as S,
    TimestampMillis,
};
use crate::log::GovernedClaimPaymentRef;
use crate::log::{
    CodecLimits, claim::response::scan as core, governed_claim::payment, reader::Reader,
};
use wf_domain::claim::apply::ClaimRejection;

pub(super) fn inspect(
    encoded: &[u8],
    time: TimestampMillis,
    admission: AdmissionLimits,
) -> Result<Reply<'_>, E> {
    if encoded.len() > admission.decision_bytes() {
        return Err(E::ByteLimit);
    }
    if time.get() > wf_domain::workflow::MAX_EXACT_MILLIS {
        return Err(E::Malformed);
    }
    let mut reader = Reader::new(encoded, CodecLimits::new(admission));
    if reader.take(6)? != b"FWAR\0\x01" {
        return Err(E::UnsupportedVersion);
    }
    let paid = reader.optional(|reader| payment::read(reader, encoded))?;
    let mut reply = Reply {
        payment: paid,
        status: S::Accepted,
        used: &[],
        phases: &[],
        phase_count: 0,
        count: 0,
        scanned: 0,
        heads: 0,
    };
    match reader.byte()? {
        0 => accepted(&mut reader, &mut reply, encoded, time, admission)?,
        1 => {
            let body = reader.raw(admission.decision_bytes())?;
            let (status, count) = core::summary(body, time, admission)?;
            if !matches!(status, S::ClaimRejected(_)) || count.is_some() {
                return Err(E::Malformed);
            }
            reply.status = status;
        }
        2 => reply.status = S::ClaimRejected(ClaimRejection::GoverningReservationUnavailable),
        _ => return Err(E::UnsupportedTag),
    }
    reader.finish()?;
    if let Some(paid) = paid {
        let scratch = encoded
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add(256))
            .ok_or(E::LengthOverflow)?;
        payment::validate(paid, scratch)?;
    }
    Ok(reply)
}

fn accepted<'a>(
    reader: &mut Reader<'a>,
    reply: &mut Reply<'a>,
    encoded: &'a [u8],
    time: TimestampMillis,
    admission: AdmissionLimits,
) -> Result<(), E> {
    let used = usize::from(reader.u16()?);
    if used > reply.payment.map_or(0, GovernedClaimPaymentRef::count) {
        return Err(E::Malformed);
    }
    reply.used = reader.take(used.checked_mul(2).ok_or(E::LengthOverflow)?)?;
    let mut previous = None;
    for pair in reply.used.as_chunks::<2>().0 {
        let n = usize::from(u16::from_be_bytes([pair[0], pair[1]]));
        if n >= reply.payment.map_or(0, GovernedClaimPaymentRef::count)
            || previous.is_some_and(|old| old >= n)
        {
            return Err(E::Malformed);
        }
        previous = Some(n);
    }
    reply.phase_count = usize::from(reader.byte()?);
    if reply.phase_count > 3 {
        return Err(E::Malformed);
    }
    let start = encoded.len() - reader.remaining();
    let mut kinds = [0; 3];
    let mut bodies: [Option<&[u8]>; 3] = [None; 3];
    let mut signature: Option<&[u8]> = None;
    for ordinal in 0..reply.phase_count {
        let kind = reader.byte()?;
        if !valid_kind(kinds, ordinal, kind) {
            return Err(E::Malformed);
        }
        kinds[ordinal] = kind;
        let body = reader.raw(admission.decision_bytes())?;
        let (status, count) = core::summary(body, time, admission)?;
        if status != S::Accepted {
            return Err(E::Malformed);
        }
        reply.count = reply
            .count
            .checked_add(count.ok_or(E::Malformed)?)
            .ok_or(E::LengthOverflow)?;
        if reply.count > 1000 || reply.count > admission.batch_items() {
            return Err(E::ItemLimit);
        }
        if count != Some(0) {
            if let Some(first) = signature
                && !core::same_phase(first, body, time, admission)?
            {
                return Err(E::Malformed);
            }
            signature = Some(body);
        }
        for previous in bodies[..ordinal].iter().flatten() {
            core::disjoint(previous, body, admission)?;
        }
        bodies[ordinal] = Some(body);
        let mut metrics = Reader::new(body, CodecLimits::new(admission));
        metrics.take(9)?;
        reply.scanned = reply
            .scanned
            .checked_add(usize::try_from(metrics.u32()?).map_err(|_| E::LengthOverflow)?)
            .ok_or(E::LengthOverflow)?;
        reply.heads = reply
            .heads
            .checked_add(usize::try_from(metrics.u32()?).map_err(|_| E::LengthOverflow)?)
            .ok_or(E::LengthOverflow)?;
    }
    let end = encoded.len() - reader.remaining();
    reply.phases = encoded.get(start..end).ok_or(E::Malformed)?;
    if reply.count > 1000
        || reply.count > admission.batch_items()
        || reply.scanned > 16_384
        || reply.heads > 4096
        || (reply.payment.is_some() && used != reply.count)
    {
        return Err(E::Malformed);
    }
    Ok(())
}

pub(super) fn valid_kind(kinds: [u8; 3], at: usize, kind: u8) -> bool {
    match at {
        0 => kind <= 2,
        1 => matches!((kinds[0], kind), (1, 2) | (2, 1)),
        2 => kinds[0] == 2 && kinds[1] == 1 && kind == 2,
        _ => false,
    }
}
