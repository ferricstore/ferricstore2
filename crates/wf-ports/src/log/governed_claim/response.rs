//! Original paid capture and complete Claim reply in one retained buffer.
use super::{E, GovernedClaimPaymentRef as P, payment};
use crate::log::{CodecLimits, claim::response as core, reader::Reader, writer::Writer};
use wf_domain::{
    claim::apply::{ClaimDecision as D, ClaimRejection as R},
    workflow::{
        admission::AdmissionLimits,
        request::{ClientIntent, RetainedStatus as S},
    },
};
use wf_types::version::TimestampMillis;

/// Complete borrowed original reply; no current paid authority or fake digest.
pub struct GovernedClaimReplyRef<'a> {
    payment: P<'a>,
    core: Option<&'a [u8]>,
    status: S,
    used: usize,
}
impl<'a> GovernedClaimReplyRef<'a> {
    /// Original complete spend, including unused suffix IDs for later settlement.
    #[must_use]
    pub const fn payment(&self) -> P<'a> {
        self.payment
    }
    /// Actual accepted postimage count.
    #[must_use]
    pub const fn used(&self) -> usize {
        self.used
    }
    /// Fully scanned original business status.
    #[must_use]
    pub const fn status(&self) -> S {
        self.status
    }
}
/// Validates all paid fields, original IDs, core response and complete EOF.
/// # Errors
/// Refuses malformed/version/profile/time/uniqueness data or resource bounds.
pub fn inspect_governed_claim_reply(
    encoded: &[u8],
    original_time: TimestampMillis,
    admission: AdmissionLimits,
) -> Result<GovernedClaimReplyRef<'_>, E> {
    if encoded.len() > admission.decision_bytes() {
        return Err(E::ByteLimit);
    }
    if original_time.get() > wf_domain::workflow::MAX_EXACT_MILLIS {
        return Err(E::Malformed);
    }
    let limits = CodecLimits::new(admission);
    let mut reader = Reader::new(encoded, limits);
    if reader.take(6)? != b"FWGC\0\x01" {
        return Err(E::UnsupportedVersion);
    }
    let payment = payment::read(&mut reader, encoded)?;
    let (core, status, used) = match reader.byte()? {
        0 => {
            let bytes = reader.raw(admission.decision_bytes())?;
            let (status, count) = core::scan::summary(bytes, original_time, admission)?;
            (Some(bytes), status, count.unwrap_or(0))
        }
        1 => (
            None,
            S::ClaimRejected(R::GoverningReservationUnavailable),
            0,
        ),
        _ => return Err(E::UnsupportedTag),
    };
    reader.finish()?;
    if used > payment.count {
        return Err(E::Malformed);
    }
    // Wire admission and uniqueness scratch are separate: a short rejected
    // reply can legitimately require more descriptor bytes than its wire size.
    // Each nonempty ID takes at least5 wire bytes and at most one slice slot.
    let scratch = encoded
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(256))
        .ok_or(E::LengthOverflow)?;
    payment::validate(payment, scratch)?;
    Ok(GovernedClaimReplyRef {
        payment,
        core,
        status,
        used,
    })
}
/// Binds the complete private paid response grammar to its retained profile.
/// # Errors
/// Refuses other profiles and malformed complete original responses.
pub fn governed_claim_retained_summary(
    intent: ClientIntent,
    version: u16,
    response: &[u8],
    time: TimestampMillis,
    admission: AdmissionLimits,
) -> Result<(S, Option<usize>), E> {
    if intent.version() != 19 || version != 16 {
        return Err(E::UnsupportedVersion);
    }
    let reply = inspect_governed_claim_reply(response, time, admission)?;
    Ok((
        reply.status,
        (reply.status == S::Accepted).then_some(reply.used),
    ))
}
/// Compares the actual operation with every original core response postimage.
/// # Errors
/// Refuses malformed complete response or invalid mathematical operation grammar.
pub fn governed_claim_response_matches(
    operation: &D<'_>,
    response: &[u8],
    time: TimestampMillis,
    admission: AdmissionLimits,
) -> Result<bool, E> {
    let reply = inspect_governed_claim_reply(response, time, admission)?;
    match (operation, reply.core) {
        (D::Rejected(R::GoverningReservationUnavailable), None) => Ok(true),
        (_, Some(bytes)) => core::claim_response_matches(operation, bytes, time, admission),
        _ => Ok(false),
    }
}
pub(in crate::log) fn measure(
    operation: &D<'_>,
    payment: P<'_>,
    admission: AdmissionLimits,
) -> Result<usize, E> {
    let mut out = Writer::measure(admission.decision_bytes());
    write(operation, payment, admission, &mut out)?;
    Ok(out.len())
}
pub(in crate::log) fn encode(
    operation: &D<'_>,
    payment: P<'_>,
    admission: AdmissionLimits,
    length: usize,
) -> Result<(Vec<u8>, S), E> {
    let mut out = Writer::allocate(length, admission.decision_bytes())?;
    let status = write(operation, payment, admission, &mut out)?;
    if out.len() != length {
        return Err(E::Malformed);
    }
    Ok((out.finish()?, status))
}
fn write(
    operation: &D<'_>,
    payment: P<'_>,
    admission: AdmissionLimits,
    out: &mut Writer<'_>,
) -> Result<S, E> {
    out.write(b"FWGC\0\x01")?;
    out.write(payment.body)?;
    if matches!(operation, D::Rejected(R::GoverningReservationUnavailable)) {
        out.byte(1)?;
        return Ok(S::ClaimRejected(R::GoverningReservationUnavailable));
    }
    if let D::Accepted(plan) = operation
        && plan.batch().map_or(0, |batch| batch.batch().items().len()) > payment.count
    {
        return Err(E::Malformed);
    }
    out.byte(0)?;
    out.u32(u32::try_from(core::measure(operation, admission)?).map_err(|_| E::LengthOverflow)?)?;
    core::write(operation, out, admission)
}
