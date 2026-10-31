//! Authorized original inquiry and supervised immutable public Claim submission.
use super::{MutationNotSubmitted, MutationSubmission};
use crate::log::{CodecLimits, PreparedBytes};
use std::sync::Arc;
use wf_domain::{
    claim::caller::NormalizedClaimDue,
    workflow::request::{ClientIntent, RequestScope},
};
use wf_types::{identity::RequestId, version::CommittedTime};

/// Server-owned cumulative bounds for one original retained inquiry.
#[derive(Clone, Copy)]
pub struct PublicClaimReadLimits {
    /// Captured original receipt grammar/admission limits.
    pub codec: CodecLimits,
    /// Open/probe/advance work ceiling.
    pub maximum_steps: usize,
    /// Cumulative owned observation bytes.
    pub maximum_bytes: usize,
    /// Cumulative adapter/key temporary bytes.
    pub maximum_temporary_bytes: usize,
}
impl Default for PublicClaimReadLimits {
    fn default() -> Self {
        Self {
            codec: CodecLimits::default(),
            maximum_steps: 10000,
            maximum_bytes: 64 * 1_048_576,
            maximum_temporary_bytes: 32 * 1_048_576,
        }
    }
}
/// Original point observation, never an applied Fresh or no-spend certificate.
pub enum PublicClaimRequestLookup {
    /// No retained identity was observed under the actual authorized lifecycle.
    Fresh,
    /// Exact original authority or a definitive/unresolved lookup diagnosis.
    Resolved(MutationSubmission),
}
/// Actual authorization, bound routing/read lifecycle and execution ownership.
/// Recheck authorization/scope at every operation, including after waits/retry.
/// Hold original bytes and permits through completion after cancellation/Unknown.
/// Implementing this interface alone establishes no production runtime.
pub trait PublicClaimSubmitPort {
    /// Resolves original identity before policy/clock/entropy; never infer no spend.
    /// # Errors
    /// Reports a current authorization/routing/read-admission refusal.
    fn lookup_public_claim(
        &mut self,
        scope: &RequestScope,
        request: &RequestId,
        intent: ClientIntent,
        limits: PublicClaimReadLimits,
    ) -> Result<PublicClaimRequestLookup, MutationNotSubmitted>;
    /// Supplies original trusted time and16-byte nonce after original inquiry.
    /// Current demand/funding is revalidated inside the applied root19 transaction.
    /// # Errors
    /// Refuses before this attempt is proposed; no child credit mutation occurs.
    fn prepare_public_claim(
        &mut self,
        scope: &RequestScope,
        request: &RequestId,
        caller: &NormalizedClaimDue<'_>,
    ) -> Result<(CommittedTime, [u8; 16]), MutationNotSubmitted>;
    /// Submits/holds this exact original owner until actual supervised completion.
    /// Resolve log replay to original retained authority or report Unknown.
    fn submit_public_claim(
        &mut self,
        scope: &RequestScope,
        request: &RequestId,
        original: Arc<PreparedBytes>,
        workspace: usize,
    ) -> MutationSubmission;
}
