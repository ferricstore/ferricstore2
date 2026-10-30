//! Explicit opt-in contracts; closed dispatch remains until verified integration.

use super::{CommittedRecoveryError as E, CommittedRecoveryLimits};
use wf_domain::governance::credit::CreditLimits;
use wf_domain::workflow::MAX_EXACT_MILLIS as MAX;

impl CommittedRecoveryLimits {
    /// Selects credit replay with the actual configured immutable shard count.
    /// # Errors
    /// Refuses invalid shard counts or a whole-call work ceiling overflow.
    pub fn with_identified_credit_limits(
        mut self,
        shard_count: u64,
        credit: CreditLimits,
    ) -> Result<Self, E> {
        if !(1..=MAX).contains(&shard_count) {
            return Err(E::InvalidLimits);
        }
        u64::try_from(self.work.0)
            .ok()
            .and_then(|n| n.checked_mul(credit.max_work()))
            .ok_or(E::Limit)?;
        self.credit = Some((shard_count, credit));
        Ok(self)
    }

    /// Reads only the explicitly selected immutable credit profile.
    #[must_use]
    pub const fn identified_credit_limits(self) -> Option<(u64, CreditLimits)> {
        self.credit
    }
}
