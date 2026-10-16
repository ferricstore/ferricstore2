//! Bounded terminal callers, prepared and submitted independently in source order.
//!
//! Authorization and supervised completion belong to the supplied submission
//! capability. These values do not establish a production runtime or durability.

use wf_domain::workflow::caller::{MutationCallerError, MutationFieldValue, MutationInput};
use wf_domain::workflow::indexed::IndexRejection;
use wf_domain::workflow::request::{ClientIntent, RequestScope};
use wf_ports::consensus::{MutationNotSubmitted, MutationSubmitPort, MutationUnknown};
use wf_ports::log::CodecLimits;
use wf_types::identity::RequestId;
use wf_types::version::{LogIdentity, TimestampMillis};

mod dispatch;
mod normalize;

/// A captured caller item with its original server-bound request namespace.
#[derive(Clone, Copy, Debug)]
pub struct MutationItemInput<'a> {
    /// Stable tenant, group and recovery namespace, not a routing hint.
    pub scope: &'a RequestScope,
    /// Caller request identity retained across retries.
    pub request: &'a RequestId,
    /// Borrowed scalar capture; richer source terms require another adapter.
    pub input: MutationInput<'a>,
}

/// Source-natural terminal families, without an aggregate durable identity.
#[derive(Clone, Copy, Debug)]
pub enum IndependentMutationCall<'a> {
    /// Normalize every item before any effect; an invalid item refuses the call.
    TerminalMany {
        /// One terminal operation that must agree with every item.
        operation: &'a str,
        /// Explicit inherited partition, or semantic automatic partitioning.
        base_partition: Option<&'a [u8]>,
        /// Common options, charged once for each original item before overrides.
        common: &'a [(&'a str, MutationFieldValue<'a>)],
        /// Original ordered caller slots.
        items: &'a [MutationItemInput<'a>],
        /// Presentation choice, excluded from each mutation's intent.
        reply: MutationReplyMode,
    },
    /// Preserve local errors and submit valid terminal items in original order.
    TerminalPipeline {
        /// Original ordered caller slots; an empty pipeline has no effects.
        items: &'a [MutationItemInput<'a>],
    },
}

/// Projection of already validated per-item outcomes, never mutation identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationReplyMode {
    /// Preserve per-item statuses.
    Items,
    /// Report scalar success only when every original many item succeeded.
    OkOnSuccess,
}

/// Positive logical budgets for one call, independently of native allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MutationCallLimits {
    /// Immutable prepared-command and core admission profile.
    pub codec: CodecLimits,
    /// Original slot ceiling, at most the source hard limit of 100,000.
    pub maximum_items: usize,
    /// Cumulative captured occurrences, identities and family-level input bytes.
    pub maximum_raw_bytes: usize,
    /// Owned result identities and fixed logical result descriptors.
    pub maximum_result_bytes: usize,
    /// Cap passed unchanged to each submission and held through its completion.
    pub execution_workspace_bytes: usize,
    /// Enclosing cap covering call owners plus preparation or execution.
    pub workspace_bytes: usize,
}

impl Default for MutationCallLimits {
    fn default() -> Self {
        Self {
            codec: CodecLimits::default(),
            maximum_items: 1_000,
            maximum_raw_bytes: 16 * 1_024 * 1_024,
            maximum_result_bytes: 16 * 1_024 * 1_024,
            execution_workspace_bytes: 256 * 1_024 * 1_024,
            workspace_bytes: 512 * 1_024 * 1_024,
        }
    }
}

/// One scalar result, preserving operational uncertainty separately from refusal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationItemStatus {
    /// Original accepted/no-op position and immutable retention deadline.
    Success {
        /// Original committed position, including on replay.
        position: LogIdentity,
        /// Original retention deadline; retries do not extend it.
        retained_until: TimestampMillis,
    },
    /// Original deterministic rejection, including its committed metadata.
    Rejected {
        /// Validated sole-item or whole-profile rejection.
        reason: IndexRejection,
        /// Original committed position.
        position: LogIdentity,
        /// Original retention deadline.
        retained_until: TimestampMillis,
    },
    /// A pipeline schema failure before preparation of this slot.
    LocalError(MutationCallerError),
    /// The request identity already belongs to a different caller intent.
    IntentConflict,
    /// The original request's retention guarantee has expired.
    Expired,
    /// The capability proves that this attempt did not reach proposal.
    NotSubmitted(MutationNotSubmitted),
    /// Work may have completed or remain supervised; later slots are stopped.
    Unknown(MutationUnknown),
}

/// Owned identity and scalar result; no input or retained-response borrow escapes.
#[derive(Debug, Eq, PartialEq)]
pub struct MutationItemResult {
    scope: RequestScope,
    request: RequestId,
    intent: Option<ClientIntent>,
    status: MutationItemStatus,
}

impl MutationItemResult {
    /// Borrows the slot's original server-bound namespace.
    #[must_use]
    pub const fn scope(&self) -> &RequestScope {
        &self.scope
    }
    /// Borrows the slot's original request identity.
    #[must_use]
    pub const fn request(&self) -> &RequestId {
        &self.request
    }
    /// Reads the actual encoded intent, absent for local/unprepared slots.
    #[must_use]
    pub const fn intent(&self) -> Option<ClientIntent> {
        self.intent
    }
    /// Borrows the scalar outcome without retaining its original response owner.
    #[must_use]
    pub const fn status(&self) -> &MutationItemStatus {
        &self.status
    }
}

/// Complete original-slot results with a separate optional scalar projection.
#[derive(Debug, Eq, PartialEq)]
pub struct IndependentMutationReply {
    items: Vec<MutationItemResult>,
    scalar_ok: bool,
}

impl IndependentMutationReply {
    /// Borrows every original slot, including local errors and stopped suffixes.
    #[must_use]
    pub fn items(&self) -> &[MutationItemResult] {
        &self.items
    }
    /// Transfers all result owners without another allocation or conversion.
    #[must_use]
    pub fn into_items(self) -> Vec<MutationItemResult> {
        self.items
    }
    /// True only for explicitly requested, nonempty, entirely successful many.
    #[must_use]
    pub const fn is_scalar_ok(&self) -> bool {
        self.scalar_ok
    }
}

/// Preflights a whole caller family, then submits individual public-V1 commands.
///
/// Every returned outer error precedes preparation/submission. Once dispatch
/// starts, per-slot statuses preserve the completed prefix; the first Unknown
/// prevents every subsequent preparation/submission. Full retained replies are
/// validated and dropped before preparing the next slot.
///
/// # Errors
/// Refuses invalid profiles, raw/count/result/workspace limits, allocation and
/// whole-many normalization or preparation-measurement errors before effects.
pub fn submit_independent_mutations<P: MutationSubmitPort>(
    call: IndependentMutationCall<'_>,
    port: &mut P,
    limits: MutationCallLimits,
) -> Result<IndependentMutationReply, MutationCallerError> {
    let scalar_requested = matches!(
        call,
        IndependentMutationCall::TerminalMany {
            reply: MutationReplyMode::OkOnSuccess,
            ..
        }
    );
    let mut callers = Vec::new();
    let mut items = Vec::new();
    let global = normalize::prepare(call, limits, &mut callers, &mut items)?;
    dispatch::run(&callers, &mut items, port, limits, global);
    let scalar_ok = scalar_requested
        && !items.is_empty()
        && items
            .iter()
            .all(|item| matches!(item.status, MutationItemStatus::Success { .. }));
    Ok(IndependentMutationReply { items, scalar_ok })
}
