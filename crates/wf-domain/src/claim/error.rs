//! Selection admission, authoritative-view and host resource failure signals.

use std::fmt::{Display, Formatter};

/// A whole-call rejection; no partial successful selection is returned.
///
/// These signals are not durable workflow outcomes. A future replicated apply
/// adapter must distinguish request admission from failed authoritative-view
/// validation and host resource failure. Invalid canonical/index contents require
/// corruption/failure handling; allocation failure requires host recovery or retry.
/// Neither may be persisted as a deterministic workflow business rejection.
/// `InvalidPriority` can arise from the request or an authoritative row, so the
/// validation context also matters when classifying a failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionError {
    /// Scope/filter components are empty, oversized or duplicated.
    InvalidScope,
    /// A source priority is outside 0 through 2.
    InvalidPriority,
    /// Claim/output or scan/head limits exceed the bounded selection contract.
    InvalidBudget,
    /// The authoritative group differs from the committed request.
    GroupMismatch,
    /// The view contains more lanes than can be globally ordered under the head budget.
    HeadBudget,
    /// A continuation belongs to another request or explicit snapshot identity.
    StaleContinuation,
    /// A consumed index key/metadata disagrees with its authoritative record.
    InvalidIndex,
    /// A FIFO lane has a nonzero priority or also appears in the parallel index.
    InvalidFifo,
    /// Two consumed rows refer to the same partition/record identity.
    DuplicateIdentity,
    /// Host resource failure: bounded result/frontier allocation could not be reserved.
    ///
    /// Replicas can differ in available memory. Apply must not persist this signal
    /// as a workflow outcome or skip the committed decision because of it.
    AllocationFailed,
}

impl Display for SelectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidScope => "invalid claim selection scope",
            Self::InvalidPriority => "claim priority must be between 0 and 2",
            Self::InvalidBudget => "claim selection budget exceeds supported bounds",
            Self::GroupMismatch => "claim selection group mismatch",
            Self::HeadBudget => "claim selection requires more authoritative lane heads",
            Self::StaleContinuation => {
                "claim selection continuation does not match this snapshot/request"
            }
            Self::InvalidIndex => "claim index disagrees with authoritative record",
            Self::InvalidFifo => "invalid authoritative fifo index placement or priority",
            Self::DuplicateIdentity => "duplicate authoritative claim identity",
            Self::AllocationFailed => "claim selection allocation failed",
        })
    }
}

impl std::error::Error for SelectionError {}
