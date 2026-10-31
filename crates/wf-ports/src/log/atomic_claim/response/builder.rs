//! Bounded actual phase-postimage construction; this data proves no commit.

use super::{
    AdmissionLimits, CodecError as E, GovernedClaimPaymentRef as Payment, RetainedStatus as Status,
};
use crate::log::{claim::response as core, writer::Writer};
use std::borrow::Cow;
use wf_domain::claim::{
    apply::{ClaimDecision, ClaimRejection},
    coordination::ClaimPhase,
};

/// Original payment and complete ordered phase receipts, without publication authority.
pub struct AtomicClaimReplyBuilder<'a> {
    payment: Option<Cow<'a, [u8]>>,
    payment_count: usize,
    admission: AdmissionLimits,
    maximum_owned: usize,
    phases: Vec<u8>,
    kinds: [u8; 3],
    phase_count: usize,
    used: Vec<u16>,
    count: usize,
    scanned: usize,
    heads: usize,
}
impl<'a> AtomicClaimReplyBuilder<'a> {
    /// Reserves original-ID position ownership before any phase effects.
    /// # Errors
    /// Refuses represented owner limits or failed fallible reservation.
    pub fn new(
        payment: Option<Payment<'a>>,
        admission: AdmissionLimits,
        maximum_owned: usize,
    ) -> Result<Self, E> {
        Self::build(
            payment.map(|paid| Cow::Borrowed(paid.encoded())),
            payment.map_or(0, Payment::count),
            admission,
            maximum_owned,
        )
    }

    /// Admits complete immutable original payment ownership without ID copies.
    /// Encoding alone never certifies spend or current ownership.
    /// # Errors
    /// Refuses malformed payment or configured byte/scratch/owner limits.
    pub fn new_owned_payment(
        encoded: Vec<u8>,
        admission: AdmissionLimits,
        maximum_owned: usize,
    ) -> Result<Self, E> {
        let remaining = maximum_owned
            .checked_sub(encoded.len())
            .and_then(|n| n.checked_sub(4096))
            .ok_or(E::OwnedLimit)?;
        let count = crate::log::inspect_claim_payment(
            &encoded,
            crate::log::CodecLimits::new(admission),
            remaining,
        )?
        .count();
        Self::build(Some(Cow::Owned(encoded)), count, admission, maximum_owned)
    }

    fn build(
        payment: Option<Cow<'a, [u8]>>,
        count: usize,
        admission: AdmissionLimits,
        maximum_owned: usize,
    ) -> Result<Self, E> {
        let fixed = payment
            .as_ref()
            .map_or(0, |paid| paid.len())
            .checked_add(4096)
            .and_then(|n| count.checked_mul(2).and_then(|m| n.checked_add(m)))
            .ok_or(E::LengthOverflow)?;
        if fixed > maximum_owned {
            return Err(E::OwnedLimit);
        }
        let mut used = Vec::new();
        used.try_reserve_exact(count).map_err(|_| E::Allocation)?;
        Ok(Self {
            payment,
            payment_count: count,
            admission,
            maximum_owned,
            phases: Vec::new(),
            kinds: [0; 3],
            phase_count: 0,
            used,
            count: 0,
            scanned: 0,
            heads: 0,
        })
    }

    /// Borrows the immutable original payment without producing a live witness.
    /// # Errors
    /// Refuses inconsistent internal bytes; original full uniqueness is immutable.
    pub fn payment(&self) -> Result<Option<Payment<'_>>, E> {
        let Some(bytes) = self.payment.as_deref() else {
            return Ok(None);
        };
        let mut reader =
            crate::log::reader::Reader::new(bytes, crate::log::CodecLimits::new(self.admission));
        let paid = crate::log::governed_claim::payment::read(&mut reader, bytes)?;
        reader.finish()?;
        if paid.count() != self.payment_count {
            return Err(E::Malformed);
        }
        Ok(Some(paid))
    }

    /// Reads accumulated actual candidate/head and accepted-write counts.
    #[must_use]
    pub const fn progress(&self) -> (usize, usize, usize) {
        (self.scanned, self.heads, self.count)
    }

    /// Exact original positions actually attached to accepted record postimages.
    #[must_use]
    pub fn used_indices(&self) -> &[u16] {
        &self.used
    }

    /// Appends complete accepted postimages with actual original used positions.
    /// Positions must be monotone across phases; skipped IDs may create gaps.
    /// # Errors
    /// Refuses rejection/prefix mixing, bad phase/IDs, counts, bytes or ownership.
    pub fn push(
        &mut self,
        kind: ClaimPhase,
        decision: &ClaimDecision<'_>,
        used: &[u16],
    ) -> Result<(), E> {
        let kind = match kind {
            ClaimPhase::Requested => 0,
            ClaimPhase::Normal => 1,
            ClaimPhase::Running => 2,
        };
        if !super::scan::valid_kind(self.kinds, self.phase_count, kind) {
            return Err(E::Malformed);
        }
        let ClaimDecision::Accepted(plan) = decision else {
            return Err(E::Malformed);
        };
        let count = plan.batch().map_or(0, |batch| batch.batch().items().len());
        if used.len() != if self.payment.is_some() { count } else { 0 } {
            return Err(E::Malformed);
        }
        let mut previous = self.used.last().copied();
        for &at in used {
            if usize::from(at) >= self.payment_count || previous.is_some_and(|old| old >= at) {
                return Err(E::Malformed);
            }
            previous = Some(at);
        }
        let count = self.count.checked_add(count).ok_or(E::LengthOverflow)?;
        let scanned = self
            .scanned
            .checked_add(plan.scanned())
            .ok_or(E::LengthOverflow)?;
        let heads = self
            .heads
            .checked_add(plan.head_reads())
            .ok_or(E::LengthOverflow)?;
        if count > 1000 || count > self.admission.batch_items() || scanned > 16_384 || heads > 4096
        {
            return Err(E::ItemLimit);
        }
        let length = core::measure(decision, self.admission)?;
        let next = self
            .phases
            .len()
            .checked_add(length)
            .and_then(|n| n.checked_add(5))
            .ok_or(E::LengthOverflow)?;
        self.admit(next, length)?;
        self.phases
            .try_reserve_exact(length.checked_add(5).ok_or(E::LengthOverflow)?)
            .map_err(|_| E::Allocation)?;
        let mut body = Writer::allocate(length, self.admission.decision_bytes())?;
        if core::write(decision, &mut body, self.admission)? != Status::Accepted {
            return Err(E::Malformed);
        }
        let body = body.finish()?;
        self.phases.push(kind);
        self.phases.extend_from_slice(
            &u32::try_from(length)
                .map_err(|_| E::LengthOverflow)?
                .to_be_bytes(),
        );
        self.phases.extend_from_slice(&body);
        self.used.extend_from_slice(used);
        self.kinds[self.phase_count] = kind;
        self.phase_count += 1;
        self.count = count;
        self.scanned = scanned;
        self.heads = heads;
        Ok(())
    }

    /// Emits one complete accepted parent body; the caller must prove phase completion.
    /// # Errors
    /// Refuses output/owner limits or allocation failure.
    pub fn finish_accepted(self) -> Result<Vec<u8>, E> {
        self.finish(None)
    }

    /// Emits rejection without any accepted prefix or used-ID declaration.
    /// This mathematical encoding proves no rollback; the real writer must abort.
    /// # Errors
    /// Refuses output/owner limits or allocation failure.
    pub fn finish_rejected(self, reason: ClaimRejection) -> Result<Vec<u8>, E> {
        self.finish(Some(reason))
    }

    fn finish(&self, reason: Option<ClaimRejection>) -> Result<Vec<u8>, E> {
        let mut measured = Writer::measure(self.admission.decision_bytes());
        self.write(&mut measured, reason)?;
        self.admit(self.phases.len(), measured.len())?;
        let mut output = Writer::allocate(measured.len(), self.admission.decision_bytes())?;
        self.write(&mut output, reason)?;
        output.finish()
    }
    fn admit(&self, phase_bytes: usize, extra: usize) -> Result<(), E> {
        let bound = phase_bytes
            .checked_mul(3)
            .and_then(|n| n.checked_add(extra))
            .and_then(|n| n.checked_add(self.payment.as_ref().map_or(0, |paid| paid.len())))
            .and_then(|n| n.checked_add(4096))
            .ok_or(E::LengthOverflow)?;
        if bound > self.maximum_owned {
            return Err(E::OwnedLimit);
        }
        Ok(())
    }
    fn write(&self, out: &mut Writer<'_>, reason: Option<ClaimRejection>) -> Result<(), E> {
        out.write(b"FWAR\0\x01")?;
        out.optional(self.payment.as_deref(), Writer::write)?;
        if let Some(reason) = reason {
            if reason == ClaimRejection::GoverningReservationUnavailable {
                return out.byte(2);
            }
            out.byte(1)?;
            let decision = ClaimDecision::Rejected(reason);
            let length = core::measure(&decision, self.admission)?;
            out.u32(u32::try_from(length).map_err(|_| E::LengthOverflow)?)?;
            core::write(&decision, out, self.admission)?;
            return Ok(());
        }
        out.byte(0)?;
        out.u16(u16::try_from(self.used.len()).map_err(|_| E::LengthOverflow)?)?;
        for at in &self.used {
            out.u16(*at)?;
        }
        out.byte(u8::try_from(self.phase_count).map_err(|_| E::LengthOverflow)?)?;
        out.write(&self.phases)
    }
}
