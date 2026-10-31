//! Original shared frame/context ownership and distinct observable phase outcomes.
use super::super::{CreditCatalogPublicationLimits, CreditCatalogRepairError};
use std::{fmt, sync::Arc};
use wf_domain::{
    governance::{
        caller::{CreditCallKind, NormalizedCreditCall},
        view::CreditSpendUnknownView,
    },
    workflow::request::{ClientIntent, RequestScope, RetainedOutcome},
};
use wf_ports::{
    consensus::{MutationNotSubmitted, MutationUnknown},
    log::{CodecError, PreparedBytes},
    state::StateTransactionError,
};
use wf_types::identity::RequestId;
/// Server-configured namespaces and one completely validated public caller.
#[derive(Clone, Copy)]
pub struct PublicCreditCallRequest<'a> {
    /// Verified credit authority namespace.
    pub source: &'a RequestScope,
    /// Configured catalog authority, in the same tenant.
    pub target: &'a RequestScope,
    /// Exact configured catalog identity.
    pub catalog: &'a [u8],
    /// Stable identity, preserved across caller retries.
    pub request: &'a RequestId,
    /// Validated public fields, without generated preparation data.
    pub caller: NormalizedCreditCall<'a>,
}
/// Whole-call prospective bounds, independent from any cached admission proof.
#[derive(Clone, Copy, Debug)]
pub struct PublicCreditCallLimits {
    /// Actual catalog/read/codec/execution profiles used by this invocation.
    pub publication: CreditCatalogPublicationLimits,
    /// Shared final frame and fixed encoder cohort ceiling.
    pub maximum_prepared_bytes: usize,
    /// Original spend-ID/context owner ceiling.
    pub maximum_ids_bytes: usize,
    /// Maximum original retained response bytes accepted from a port.
    pub maximum_response_bytes: usize,
    /// Enclosing simultaneous logical ownership ceiling.
    pub workspace_bytes: usize,
}
impl Default for PublicCreditCallLimits {
    fn default() -> Self {
        Self {
            publication: CreditCatalogPublicationLimits::default(),
            maximum_prepared_bytes: 32 * 1_048_576,
            maximum_ids_bytes: 1_048_576,
            maximum_response_bytes: 16 * 1_048_576,
            workspace_bytes: 2 * 1024 * 1_048_576,
        }
    }
}
/// Immutable original attempt, with no current authority or reusable permit.
pub struct PreparedPublicCreditCall {
    pub(super) source: RequestScope,
    pub(super) target: RequestScope,
    pub(super) request: RequestId,
    pub(super) catalog: Vec<u8>,
    pub(super) credit_scope: Vec<u8>,
    pub(super) kind: CreditCallKind,
    pub(super) shard: Option<u64>,
    pub(super) amount: Option<u64>,
    pub(super) intent: ClientIntent,
    pub(super) minimum_time: Option<u64>,
    pub(super) ids: Vec<Vec<u8>>,
    pub(super) frame: Arc<PreparedBytes>,
}
impl PreparedPublicCreditCall {
    pub(in crate::governance) fn ownership_bytes(&self) -> Option<usize> {
        let fields = [
            self.source.tenant().as_bytes(),
            self.source.group().as_bytes(),
            self.source.recovery().as_bytes(),
            self.target.tenant().as_bytes(),
            self.target.group().as_bytes(),
            self.target.recovery().as_bytes(),
            self.request.as_bytes(),
            &self.catalog,
            &self.credit_scope,
            self.frame(),
        ];
        let base = fields
            .into_iter()
            .try_fold(4096_usize, |n, b| n.checked_add(b.len()))?;
        self.ids.iter().try_fold(base, |n, id| {
            n.checked_add(size_of::<Vec<u8>>())?.checked_add(id.len())
        })
    }
    /// Original caller request identity.
    #[must_use]
    pub const fn request(&self) -> &RequestId {
        &self.request
    }
    /// Original comparison identity, excluding generated operands.
    #[must_use]
    pub const fn intent(&self) -> ClientIntent {
        self.intent
    }
    /// Exact original encoded frame, without a payload clone.
    #[must_use]
    pub fn frame(&self) -> &[u8] {
        self.frame.as_bytes()
    }
    /// Exact generated original spend IDs; empty for other operations.
    #[must_use]
    pub fn spend_ids(&self) -> &[Vec<u8>] {
        &self.ids
    }
    /// Original compensation context; never proof of a commit or completed release.
    #[must_use]
    pub fn unknown_spend(&self) -> Option<CreditSpendUnknownView<'_>> {
        let shard = self.shard?;
        (self.kind==CreditCallKind::Spend).then(||CreditSpendUnknownView{
            code:"FLOW_LIMIT_SPEND_UNKNOWN_OUTCOME",scope:&self.credit_scope,
            shard_id:shard,reservation_ids:&self.ids,
            message:"Flow limit spend outcome is unknown; release these reservation_ids before retrying",
        })
    }
}
/// Before-CREDIT-proposal failure; earlier catalog repair need not be rolled back.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicCreditCallError {
    /// Invalid configured namespaces or a required positive option.
    InvalidOptions,
    /// Prospective checked workspace/owner admission refused.
    Limit,
    /// A fallible bounded payload reservation failed.
    Allocation,
    /// Actual source/canonical read failed.
    Read(StateTransactionError),
    /// Existing-owner catalog repair was not confirmed.
    Repair(CreditCatalogRepairError),
    /// Trusted stamp/entropy preparation refused.
    Preparation(MutationNotSubmitted),
    /// Complete public framing failed before handoff.
    Codec(CodecError),
    /// Public Spend found no actual source owner.
    NotFound,
    /// Creating an absent owner requires a capacity ceiling.
    ConfigurationRequired,
}
impl fmt::Display for PublicCreditCallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions => f.write_str("invalid public credit call options"),
            Self::Limit => f.write_str("public credit call resource limit"),
            Self::Allocation => f.write_str("public credit call allocation failed"),
            Self::Read(e) => fmt::Display::fmt(e, f),
            Self::Repair(e) => fmt::Display::fmt(e, f),
            Self::Preparation(e) => write!(f, "public credit preparation refused: {e:?}"),
            Self::Codec(e) => fmt::Display::fmt(e, f),
            Self::NotFound => f.write_str("ERR flow limit not found"),
            Self::ConfigurationRequired => {
                f.write_str("ERR flow limit limit must be a non-negative integer")
            }
        }
    }
}
impl std::error::Error for PublicCreditCallError {}
/// Retry refused before submission, retaining the original compensation context.
pub struct PublicCreditRetryError {
    pub(super) error: PublicCreditCallError,
    pub(super) original: Box<PreparedPublicCreditCall>,
}
impl PublicCreditRetryError {
    /// Current preflight refusal, with no new proposal.
    #[must_use]
    pub const fn error(&self) -> PublicCreditCallError {
        self.error
    }
    /// Recovers the exact original context without copying its frame or IDs.
    #[must_use]
    pub fn into_original(self) -> Box<PreparedPublicCreditCall> {
        self.original
    }
}
impl fmt::Debug for PublicCreditRetryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}
impl fmt::Display for PublicCreditRetryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for PublicCreditRetryError {}
/// Explicit original core result, uncertainty and auxiliary publication outcome.
pub enum PublicCreditCallResult {
    /// Completely bound original accepted/business-rejected credit response.
    Retained(Box<RetainedOutcome>),
    /// Missing Get: source-compatible optional owner without a mutation.
    Missing,
    /// Missing Cleanup: zero deleted, no pending work, without a mutation.
    EmptyCleanup,
    /// The request already names another normalized public caller.
    IntentConflict,
    /// The actual retained request has expired.
    Expired,
    /// The execution owner proves this CREDIT attempt was not proposed.
    NotSubmitted(MutationNotSubmitted),
    /// Inquiry could not resolve authority; no fresh preparation followed.
    Unresolved(MutationUnknown),
    /// Submitted work may apply; preserve the exact original attempt for resolution.
    Unknown {
        /// Operational uncertainty or invalid original response.
        reason: MutationUnknown,
        /// Original immutable frame/request/IDs; resubmit without regeneration.
        original: Box<PreparedPublicCreditCall>,
    },
    /// Core credit committed, but synchronous post-commit publication is unconfirmed.
    PublicationUnconfirmed {
        /// Actual original committed credit response, never an invented rollback.
        outcome: Box<RetainedOutcome>,
        /// Current auxiliary publication diagnosis; durable outbox remains available.
        reason: CreditCatalogRepairError,
    },
}
