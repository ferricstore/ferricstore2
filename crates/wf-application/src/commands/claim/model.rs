//! Owned original submission context; no current authority or reusable permit.
use std::sync::Arc;
use wf_domain::{
    claim::caller::NormalizedClaimDue,
    workflow::request::{ClientIntent, RequestScope, RetainedOutcome},
};
use wf_ports::{
    consensus::{MutationNotSubmitted, MutationUnknown},
    log::PreparedBytes,
};
use wf_types::identity::RequestId;
mod error;
mod limits;
pub use error::{PublicClaimCallError, PublicClaimRetryError};
pub use limits::PublicClaimCallLimits;
/// Already captured caller and server-bound original namespace/request.
#[derive(Clone, Copy)]
pub struct PublicClaimCallRequest<'a> {
    /// Actual authorized original scope, rechecked by the supplied port.
    pub scope: &'a RequestScope,
    /// Original stable caller request ID.
    pub request: &'a RequestId,
    /// Full normalized caller; block/projection orchestration is separate.
    pub caller: &'a NormalizedClaimDue<'a>,
}
/// One immutable original root19 submission, retained across uncertainty.
pub struct PreparedPublicClaimCall {
    pub(super) scope: RequestScope,
    pub(super) request: RequestId,
    pub(super) intent: ClientIntent,
    pub(super) frame: Arc<PreparedBytes>,
}
impl PreparedPublicClaimCall {
    /// Exact original namespace; it grants no current authorization.
    #[must_use]
    pub const fn scope(&self) -> &RequestScope {
        &self.scope
    }
    /// Exact original stable request ID.
    #[must_use]
    pub const fn request(&self) -> &RequestId {
        &self.request
    }
    /// Original public V1 digest under the explicit internal selector21.
    #[must_use]
    pub const fn intent(&self) -> ClientIntent {
        self.intent
    }
    /// Immutable full original frame, including its trusted nonce and time.
    #[must_use]
    pub fn frame(&self) -> &[u8] {
        self.frame.as_bytes()
    }
}
/// Observable original completion or distinct definite/uncertain refusal.
pub enum PublicClaimCallResult {
    /// Fully validated original accepted or business-rejected outcome.
    Retained(Box<RetainedOutcome>),
    /// The original request belongs to another canonical caller.
    IntentConflict,
    /// Original retention expired; do not prepare another mutation for it.
    Expired,
    /// Original inquiry refused before any original frame was prepared.
    NotSubmitted(MutationNotSubmitted),
    /// Inquiry is unresolved; no preparation/submission followed.
    Unresolved(MutationUnknown),
    /// Work may apply; retain this exact immutable original for inquiry/retry.
    Unknown {
        /// Actual uncertainty or invalid returned original authority.
        reason: MutationUnknown,
        /// Same original frame and bound request; no independent credit refund.
        original: Box<PreparedPublicClaimCall>,
    },
    /// This attempt was not proposed; keep any older unresolved original context.
    Deferred {
        /// Current definite pre-proposal refusal.
        reason: MutationNotSubmitted,
        /// Unchanged original for later authorized inquiry/retry.
        original: Box<PreparedPublicClaimCall>,
    },
}
