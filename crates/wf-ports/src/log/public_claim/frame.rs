//! Explicit public funded profile; older atomic profiles remain closed.
use super::{CodecError, NormalizedClaimDue, PreparedPublicClaim, PublicClaimPreparation};
use crate::log::{
    AtomicClaimOptions, CodecLimits, PreparedBytes,
    atomic_claim::frame::{FrameProfile, encode_profile, prepare_profile, recovery_profile},
};
use wf_domain::{claim::caller::ClaimCallerLimits, workflow::request::RequestScope};
use wf_types::{identity::RequestId, version::CommittedTime};

/// Borrows exact public parent recovery routing facts; full audit is still required.
/// # Errors
/// Refuses other profiles, malformed headers and original-byte admission.
pub fn public_claim_recovery_header(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<(u16, [&[u8]; 3], CommittedTime), CodecError> {
    recovery_profile(encoded, limits, FrameProfile::PublicFunded)
}

/// Encodes the original public caller and actual generated preparation.
/// # Errors
/// Refuses selected caller, framing or prospective byte/owner admission.
pub fn encode_public_claim_request(
    scope: &RequestScope,
    request: &RequestId,
    stamp: CommittedTime,
    caller: &NormalizedClaimDue<'_>,
    preparation: PublicClaimPreparation,
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedBytes, CodecError> {
    encode_profile(
        scope,
        request,
        stamp,
        caller,
        (
            AtomicClaimOptions {
                token_version: preparation.token_version,
                first_ordinal: preparation.first_ordinal,
                scan_budget: preparation.scan_budget,
                head_budget: preparation.head_budget,
                payment: None,
            },
            Some(preparation.batch),
        ),
        limits,
        maximum_owned,
    )
}

/// Fully prepares actual op19/intent21 bytes before any current-state access.
/// # Errors
/// Refuses complete grammar, canonical caller, public V1 digest or limits.
pub fn prepare_public_claim_request(
    encoded: &[u8],
    limits: CodecLimits,
    caller: ClaimCallerLimits,
    maximum_owned: usize,
) -> Result<PreparedPublicClaim<'_>, CodecError> {
    Ok(PreparedPublicClaim {
        inner: prepare_profile(
            encoded,
            limits,
            caller,
            maximum_owned,
            FrameProfile::PublicFunded,
        )?,
    })
}
