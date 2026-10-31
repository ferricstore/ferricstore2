//! Prospective original frame, inquiry, reply and execution ownership.
use wf_domain::claim::{caller::ClaimCallerLimits, coordination::ClaimCoordinationLimits};
use wf_ports::{consensus::PublicClaimReadLimits, log::PublicClaimReplyLimits};
/// Positive server profiles, not client-controlled increases or cached permits.
#[derive(Clone, Copy)]
pub struct PublicClaimCallLimits {
    /// Current retained read/codec profile.
    pub read: PublicClaimReadLimits,
    /// Complete returned caller/payment/phase binding profile.
    pub reply: PublicClaimReplyLimits,
    /// Current prepared caller descriptor admission.
    pub caller: ClaimCallerLimits,
    /// Canonical V1 byte ceiling before any effect.
    pub maximum_intent_bytes: usize,
    /// Shared final frame/encoder or concurrent parser owner ceiling.
    pub maximum_prepared_bytes: usize,
    /// Original complete retained response owner ceiling.
    pub maximum_response_bytes: usize,
    /// Supervised execution ownership, held until actual completion.
    pub execution_workspace_bytes: usize,
    /// Enclosing simultaneous logical ownership ceiling.
    pub workspace_bytes: usize,
    /// Server-selected deterministic lease token format.
    pub token_version: u16,
    /// First original output ordinal.
    pub first_ordinal: u32,
    /// Whole original candidate-scan budget.
    pub scan_budget: usize,
    /// Whole original lane-head budget.
    pub head_budget: usize,
}
impl Default for PublicClaimCallLimits {
    fn default() -> Self {
        let read = PublicClaimReadLimits::default();
        Self {
            reply: PublicClaimReplyLimits {
                admission: read.codec.admission(),
                coordination: ClaimCoordinationLimits::default(),
                credit_shard_count: 4,
            },
            read,
            caller: ClaimCallerLimits::default(),
            maximum_intent_bytes: 1_048_576,
            maximum_prepared_bytes: 32 * 1_048_576,
            maximum_response_bytes: 16 * 1_048_576,
            execution_workspace_bytes: 512 * 1_048_576,
            workspace_bytes: 2 * 1024 * 1_048_576,
            token_version: 1,
            first_ordinal: 0,
            scan_budget: 16_384,
            head_budget: 4096,
        }
    }
}
