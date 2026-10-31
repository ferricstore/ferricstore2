//! Public V1 Claim comparison over normalized original caller data only.
use super::CodecError;
use wf_domain::{claim::caller::NormalizedClaimDue, workflow::request::ClientIntent};
use wf_types::identity::TenantId;
mod binding;
mod frame;
pub(in crate::log) mod intent;
mod model;
mod response;
pub use binding::inspect_public_claim_reply_for_call;
pub use frame::{
    encode_public_claim_request, prepare_public_claim_request, public_claim_recovery_header,
};
pub use model::{PreparedPublicClaim, PublicClaimPreparation, PublicClaimReplyLimits};
pub use response::{
    PublicClaimReplyKind, PublicClaimReplyRef, encode_public_claim_funding_reply,
    encode_public_claim_reply, inspect_public_claim_reply, public_claim_retained_summary,
};

/// Streams the existing public V1 registry without preparing time, IDs or a frame.
/// Mathematical comparison grants no routing, spend or durable execution authority.
/// # Errors
/// Refuses checked size/descriptor admission before hashing original leaves.
pub fn prepare_public_claim_intent(
    tenant: &TenantId,
    caller: &NormalizedClaimDue<'_>,
    maximum_bytes: usize,
) -> Result<ClientIntent, CodecError> {
    intent::prepare(tenant.as_bytes(), caller, maximum_bytes)
}
