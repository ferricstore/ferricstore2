//! Full public-parent reply grammar; data encoding proves no funding or `NoDemand`.
use crate::log::{AtomicClaimReplyRef, CodecError};
use crate::log::{
    CodecLimits, credit::status, inspect_atomic_claim_reply, reader::Reader, writer::Writer,
};
use wf_domain::{
    governance::identified::IdentifiedCreditRejection,
    workflow::{
        admission::AdmissionLimits,
        request::{ClientIntent, RetainedStatus},
    },
};
use wf_types::version::TimestampMillis;

/// Distinguishes actual Claim selection from failed-funding physical emptiness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicClaimReplyKind {
    /// Actual accepted or rejected Claim execution.
    Claim,
    /// Actual failed spending followed by physically empty relevant due ranges.
    NoDemand,
    /// Actual funding refused while emptiness was not established.
    FundingRejected,
}

/// Completely inspected original reply; it supplies no current authority.
pub struct PublicClaimReplyRef<'a> {
    kind: PublicClaimReplyKind,
    status: RetainedStatus,
    claim: Option<AtomicClaimReplyRef<'a>>,
    failed_credit: Option<IdentifiedCreditRejection>,
}
impl<'a> PublicClaimReplyRef<'a> {
    /// Actual original reply branch.
    #[must_use]
    pub const fn kind(&self) -> PublicClaimReplyKind {
        self.kind
    }
    /// Fully validated business classification.
    #[must_use]
    pub const fn status(&self) -> RetainedStatus {
        self.status
    }
    /// Complete original phase/payment data, when Claim execution was reached.
    #[must_use]
    pub const fn claim(&self) -> Option<&AtomicClaimReplyRef<'a>> {
        self.claim.as_ref()
    }
    /// Original actual failed-credit diagnosis, without an owner snapshot.
    #[must_use]
    pub const fn failed_credit(&self) -> Option<IdentifiedCreditRejection> {
        self.failed_credit
    }
}

/// Inspects all original nested Claim/payment/rejection bytes and EOF.
/// # Errors
/// Refuses malformed profiles, original times, discriminants or configured bounds.
pub fn inspect_public_claim_reply(
    response: &[u8],
    time: TimestampMillis,
    admission: AdmissionLimits,
) -> Result<PublicClaimReplyRef<'_>, CodecError> {
    if response.len() > admission.decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    if time.get() > wf_domain::workflow::MAX_EXACT_MILLIS {
        return Err(CodecError::Malformed);
    }
    let mut reader = Reader::new(response, CodecLimits::new(admission));
    if reader.take(6)? != b"FWFR\0\x01" {
        return Err(CodecError::UnsupportedVersion);
    }
    let reply = match reader.byte()? {
        0 => {
            let claim = inspect_atomic_claim_reply(
                reader.raw(admission.decision_bytes())?,
                time,
                admission,
            )?;
            PublicClaimReplyRef {
                kind: PublicClaimReplyKind::Claim,
                status: claim.status(),
                claim: Some(claim),
                failed_credit: None,
            }
        }
        kind @ (1 | 2) => {
            let reason = status::read_rejection(&mut reader)?;
            PublicClaimReplyRef {
                kind: if kind == 1 {
                    PublicClaimReplyKind::NoDemand
                } else {
                    PublicClaimReplyKind::FundingRejected
                },
                status: if kind == 1 {
                    RetainedStatus::Accepted
                } else {
                    RetainedStatus::GovernanceRejected(reason)
                },
                claim: None,
                failed_credit: Some(reason),
            }
        }
        _ => return Err(CodecError::UnsupportedTag),
    };
    reader.finish()?;
    Ok(reply)
}

/// Encodes a complete actual Claim response without changing its original data.
/// # Errors
/// Refuses malformed nested bytes or prospective output/owner limits.
pub fn encode_public_claim_reply(
    claim: &[u8],
    time: TimestampMillis,
    admission: AdmissionLimits,
    maximum_owned: usize,
) -> Result<Vec<u8>, CodecError> {
    inspect_atomic_claim_reply(claim, time, admission)?;
    encode(0, Some(claim), None, admission, maximum_owned)
}

/// Encodes a known funding failure, optionally classified by real physical emptiness.
/// Encoding supplies no spend, current-owner or physical-presence authority.
/// # Errors
/// Refuses invalid rejection values or prospective output/owner limits.
pub fn encode_public_claim_funding_reply(
    reason: IdentifiedCreditRejection,
    no_demand: bool,
    admission: AdmissionLimits,
    maximum_owned: usize,
) -> Result<Vec<u8>, CodecError> {
    encode(
        if no_demand { 1 } else { 2 },
        None,
        Some(reason),
        admission,
        maximum_owned,
    )
}

fn encode(
    kind: u8,
    claim: Option<&[u8]>,
    reason: Option<IdentifiedCreditRejection>,
    admission: AdmissionLimits,
    maximum_owned: usize,
) -> Result<Vec<u8>, CodecError> {
    let write = |out: &mut Writer<'_>| {
        out.write(b"FWFR\0\x01")?;
        out.byte(kind)?;
        if let Some(claim) = claim {
            out.raw(claim, admission.decision_bytes())?;
        }
        if let Some(reason) = reason {
            status::rejection(out, reason)?;
        }
        Ok(())
    };
    let mut measured = Writer::measure(admission.decision_bytes());
    write(&mut measured)?;
    if measured
        .len()
        .checked_add(4096)
        .ok_or(CodecError::LengthOverflow)?
        > maximum_owned
    {
        return Err(CodecError::OwnedLimit);
    }
    let mut out = Writer::allocate(measured.len(), admission.decision_bytes())?;
    write(&mut out)?;
    out.finish()
}

/// Associates only the explicitly selected public funded parent with response18.
/// # Errors
/// Refuses other profiles or any malformed complete original reply.
pub fn public_claim_retained_summary(
    intent: ClientIntent,
    version: u16,
    response: &[u8],
    time: TimestampMillis,
    admission: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), CodecError> {
    if intent.version() != 21 || version != 18 {
        return Err(CodecError::UnsupportedVersion);
    }
    let reply = inspect_public_claim_reply(response, time, admission)?;
    Ok((
        reply.status,
        (reply.status == RetainedStatus::Accepted)
            .then_some(reply.claim.as_ref().map_or(0, AtomicClaimReplyRef::count)),
    ))
}
