//! Original-caller atomic Claim framing; local publication needs the real driver.

use super::{CodecError, CodecLimits, GovernedClaimPayment, PreparedBytes};
use wf_domain::claim::caller::NormalizedClaimDue;
use wf_domain::workflow::request::RequestScope;
use wf_types::{identity::RequestId, version::CommittedTime};

mod caller;
mod frame;
mod model;
mod phase;
mod policy;
mod response;
pub use frame::{
    atomic_claim_recovery_header, encode_atomic_claim_request, prepare_atomic_claim_request,
};
pub use model::PreparedAtomicClaim;
pub use phase::{AtomicClaimInspection, FreshAtomicClaim, inspect_atomic_claim_point};
pub use policy::running_claim_scope;
pub use response::{
    AtomicClaimReplyBuilder, AtomicClaimReplyRef, atomic_claim_retained_summary,
    inspect_atomic_claim_reply,
};

/// Operational committed inputs, separate from original caller intent.
#[derive(Clone, Copy)]
pub struct AtomicClaimOptions<'a> {
    /// Captured source token algorithm version.
    pub token_version: u16,
    /// First available state-entry token ordinal.
    pub first_ordinal: u32,
    /// Total candidate-scan allowance for the entire parent.
    pub scan_budget: usize,
    /// Total lane-head allowance for the entire parent.
    pub head_budget: usize,
    /// Exact original spent identities; this value proves no remote ownership.
    pub payment: Option<GovernedClaimPayment<'a>>,
}
