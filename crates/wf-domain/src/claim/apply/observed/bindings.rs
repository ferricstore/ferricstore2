//! A bounded original-batch cursor; no paid-credit or completeness authority.
use super::{AdmissionLimits, ApplyError, ClaimPreflightError, ClaimRejection, ClaimWorkLimits};
use crate::workflow::WorkflowRecord;

pub(super) struct ClaimBindingCursor {
    maximum: usize,
    attempts: usize,
    positions: Vec<usize>,
}
impl ClaimBindingCursor {
    pub(super) fn new(
        maximum: usize,
        limit: usize,
        query_bytes: &mut usize,
        work_bytes: &mut u64,
        admission: AdmissionLimits,
        work: ClaimWorkLimits,
    ) -> Result<Self, ClaimPreflightError> {
        let overflow = ClaimPreflightError::Business(ClaimRejection::LengthOverflow);
        let bytes = limit
            .checked_mul(std::mem::size_of::<usize>())
            .ok_or(overflow)?;
        *query_bytes = query_bytes.checked_add(bytes).ok_or(overflow)?;
        admission
            .charge(*query_bytes, 0)
            .map_err(|_| ClaimPreflightError::Business(ClaimRejection::DecisionBytes))?;
        let total = work_bytes
            .checked_add(u64::try_from(bytes).map_err(|_| overflow)?)
            .ok_or(overflow)?;
        if total > work.max_work_bytes() {
            return Err(ClaimPreflightError::Business(ClaimRejection::WorkBytes {
                bytes: total,
                maximum: work.max_work_bytes(),
            }));
        }
        *work_bytes = total;
        let mut positions = Vec::new();
        positions
            .try_reserve_exact(limit)
            .map_err(|_| ClaimPreflightError::Apply(ApplyError::AllocationFailed))?;
        Ok(Self {
            maximum,
            attempts: 0,
            positions,
        })
    }
    pub(super) fn bind(&mut self, record: &WorkflowRecord) -> Result<bool, ClaimPreflightError> {
        if self.attempts >= self.maximum {
            return Err(ClaimPreflightError::Business(
                ClaimRejection::GoverningReservationUnavailable,
            ));
        }
        let position = self.attempts;
        self.attempts = self
            .attempts
            .checked_add(1)
            .ok_or(ClaimPreflightError::Business(
                ClaimRejection::LengthOverflow,
            ))?;
        // The source binds first, then QueryRowPrimitives rejects this integer.
        // Ordinary Claim/Reclaim preserve attempts; private u64 admission remains valid.
        if i64::try_from(record.attempts()).is_err() {
            return Ok(false);
        }
        self.positions.push(position);
        Ok(true)
    }
    pub(super) fn into_ledger(self) -> (usize, Vec<usize>) {
        (self.attempts, self.positions)
    }
}
