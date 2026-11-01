//! One continuous policy work/scratch account with prospective allocation refusal.
use super::{PolicySchemaError, PolicySchemaLimits};

/// Continuous logical work and peak scratch for one parent-owned account.
///
/// Matching profiles do not prove accounting identity. The authoritative parent
/// must thread the same account through all operations; there is no reset/debit API.
/// Input construction, refused-owner destruction and native capacity/CPU/RSS are
/// outside this represented estimate and require bounded outer preparation.
pub struct PolicySchemaCharge {
    limits: PolicySchemaLimits,
    g: u64,
    p: u64,
    maximum_scratch: u64,
}

impl PolicySchemaCharge {
    /// Begins one empty account under the immutable supplied profile.
    #[must_use]
    pub const fn new(limits: PolicySchemaLimits) -> Self {
        Self {
            limits,
            g: 0,
            p: 0,
            maximum_scratch: u64::MAX,
        }
    }

    /// Select a prospective scratch ceiling inside the same work/schema account.
    /// Zero permits only inspection needing no auxiliary scratch allocation.
    #[must_use]
    pub const fn with_scratch_limit(limits: PolicySchemaLimits, maximum_scratch: u64) -> Self {
        Self {
            limits,
            g: 0,
            p: 0,
            maximum_scratch,
        }
    }

    /// Reads admitted cumulative occurrence work.
    #[must_use]
    pub const fn g(&self) -> u64 {
        self.g
    }

    /// Reads admitted peak logical scratch, retained after temporary sets drop.
    #[must_use]
    pub const fn p(&self) -> u64 {
        self.p
    }

    /// Reads the sum whose nonoverflowing invariant each successful debit proves.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.g + self.p
    }

    /// Reads the selected profile without validation, credit or authority.
    #[must_use]
    pub const fn profile(&self) -> PolicySchemaLimits {
        self.limits
    }
}

pub(crate) fn charge_schema(
    charge: &mut PolicySchemaCharge,
    g: u64,
    scratch_p: u64,
) -> Result<(), PolicySchemaError> {
    let next_g = charge
        .g
        .checked_add(g)
        .ok_or(PolicySchemaError::LengthOverflow)?;
    let next_p = charge.p.max(scratch_p);
    let bytes = next_g
        .checked_add(next_p)
        .ok_or(PolicySchemaError::LengthOverflow)?;
    if next_p > charge.maximum_scratch {
        return Err(PolicySchemaError::WorkBytes {
            bytes,
            maximum: next_g
                .checked_add(charge.maximum_scratch)
                .ok_or(PolicySchemaError::LengthOverflow)?
                .min(charge.limits.work_bytes),
        });
    }
    if bytes > charge.limits.work_bytes {
        return Err(PolicySchemaError::WorkBytes {
            bytes,
            maximum: charge.limits.work_bytes,
        });
    }
    charge.g = next_g;
    charge.p = next_p;
    Ok(())
}
