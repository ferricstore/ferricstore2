//! Whole-parent response grammar, independent of native commit authority.
use super::super::{CodecError, GovernedClaimPaymentRef};
use wf_domain::workflow::{
    admission::AdmissionLimits,
    request::{ClientIntent, RetainedStatus},
};
use wf_types::version::TimestampMillis;
mod builder;
mod scan;
pub use builder::AtomicClaimReplyBuilder;

/// Completely inspected parent response; it is no live transaction certificate.
pub struct AtomicClaimReplyRef<'a> {
    pub(super) payment: Option<GovernedClaimPaymentRef<'a>>,
    pub(super) status: RetainedStatus,
    pub(super) used: &'a [u8],
    pub(super) phases: &'a [u8],
    pub(super) phase_count: usize,
    pub(super) count: usize,
    pub(super) scanned: usize,
    pub(super) heads: usize,
}
impl<'a> AtomicClaimReplyRef<'a> {
    /// Original payment, retaining every reserved identity including unused IDs.
    #[must_use]
    pub const fn payment(&self) -> Option<GovernedClaimPaymentRef<'a>> {
        self.payment
    }
    /// Fully inspected original business status.
    #[must_use]
    pub const fn status(&self) -> RetainedStatus {
        self.status
    }
    /// Number of accepted postimages across all actual phase receipts.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.count
    }
    /// Number of complete accepted phase receipts.
    #[must_use]
    pub const fn phase_count(&self) -> usize {
        self.phase_count
    }
    /// Complete original nested phase bytes.
    #[must_use]
    pub const fn phases(&self) -> &'a [u8] {
        self.phases
    }
    /// Total actual nested candidate scans and lane-head reads.
    #[must_use]
    pub const fn work_counts(&self) -> (usize, usize) {
        (self.scanned, self.heads)
    }
    /// Monotone original reservation positions, including non-prefix use.
    pub fn used_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.used
            .as_chunks::<2>()
            .0
            .iter()
            .map(|bytes| usize::from(u16::from_be_bytes([bytes[0], bytes[1]])))
    }
    /// Used original reservation bytes in original spend order, without ID copies.
    pub fn used_reservation_ids(&self) -> impl Iterator<Item = Result<&'a [u8], CodecError>> + '_ {
        let mut indices = self.used_indices();
        let mut next = indices.next();
        self.payment
            .into_iter()
            .flat_map(GovernedClaimPaymentRef::reservation_ids)
            .enumerate()
            .filter_map(move |(at, id)| {
                if next == Some(at) {
                    next = indices.next();
                    Some(id)
                } else {
                    None
                }
            })
    }
}

/// Scans full parent payment, postimages, original time and EOF.
/// # Errors
/// Refuses malformed phases/payment, non-monotone used IDs or configured limits.
pub fn inspect_atomic_claim_reply(
    response: &[u8],
    original_time: TimestampMillis,
    admission: AdmissionLimits,
) -> Result<AtomicClaimReplyRef<'_>, CodecError> {
    scan::inspect(response, original_time, admission)
}

/// Associates only intent20/response17 with the whole-parent reply grammar.
/// # Errors
/// Refuses other profiles or any malformed original response.
pub fn atomic_claim_retained_summary(
    intent: ClientIntent,
    version: u16,
    response: &[u8],
    time: TimestampMillis,
    admission: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), CodecError> {
    if intent.version() != 20 || version != 17 {
        return Err(CodecError::UnsupportedVersion);
    }
    let reply = inspect_atomic_claim_reply(response, time, admission)?;
    Ok((
        reply.status,
        (reply.status == RetainedStatus::Accepted).then_some(reply.count),
    ))
}
