//! Original byte references; physical reads and runtime capture stay external.

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    OriginalEtfSafety, OriginalPolicyReadError, OriginalPolicyReadValue, PolicySchemaCharge,
    PolicySchemaLimits, prepare_original_complete_policy_reference_bytes,
};
use wf_domain::workflow::control::PreparedDigest;

use crate::log::PreparedBytes;

/// Original policy generation and exact bytes, with the requested type borrowed.
///
/// This owner grants no storage, current-policy or runtime-context authority.
/// Present bytes preserve their original ETF encoding; absence uses the original
/// generation-zero type-map fallback, distinct from public Policy absence.
pub struct OriginalPolicyReference<'query> {
    requested_type: &'query [u8],
    generation: u64,
    prepared: PreparedBytes,
}

impl<'query> OriginalPolicyReference<'query> {
    /// Borrows the original query, independently of source and safety lifetimes.
    #[must_use]
    pub const fn workflow_type(&self) -> &'query [u8] {
        self.requested_type
    }

    /// Reads the original generation, including zero.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Borrows exact prepared bytes without canonicalizing the original encoding.
    #[must_use]
    pub fn encoded(&self) -> &[u8] {
        self.prepared.as_bytes()
    }

    /// Reads SHA256 over the exact owned bytes.
    #[must_use]
    pub const fn digest(&self) -> PreparedDigest {
        self.prepared.digest()
    }

    /// Moves the original generation, query borrow and prepared byte owner.
    #[must_use]
    pub fn into_parts(self) -> (&'query [u8], u64, PreparedBytes) {
        (self.requested_type, self.generation, self.prepared)
    }
}

/// Prepares and hashes an original byte reference on the received work account.
///
/// Raw read classes are freshly checked; no accepted entry or prepaid token is
/// accepted. Pure domain preparation prospectively pays copy/emission, SHA and
/// reference metadata before this private outer SHA computation. Native hash
/// execution on a late refusal is not implied by that conservative schedule.
///
/// # Errors
/// Preserves source read/type errors and operational profile, work, allocation,
/// checked-length and unsupported-runtime errors without a partial reference.
pub fn prepare_original_complete_policy_reference<'query>(
    value: OriginalPolicyReadValue<'_>,
    requested_type: &'query [u8],
    safety: &OriginalEtfSafety<'_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<OriginalPolicyReference<'query>, OriginalPolicyReadError> {
    let (generation, bytes) = prepare_original_complete_policy_reference_bytes(
        value,
        requested_type,
        safety,
        limits,
        charge,
    )?;
    let digest = PreparedDigest::new(Sha256::digest(&bytes).into());
    Ok(OriginalPolicyReference {
        requested_type,
        generation,
        prepared: PreparedBytes { bytes, digest },
    })
}
