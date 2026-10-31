//! Fixed startup diagnoses shared by committed and snapshot recovery.

use super::*;
use std::fmt;

/// Fixed startup failures; every recovery error drops both database handles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommittedRecoveryError {
    /// A required positive profile or point-owner minimum is invalid.
    InvalidLimits,
    /// A whole-call work/owner allowance or checked arithmetic was exceeded.
    Limit,
    /// The independently supplied namespace differs from actual store/header scope.
    NamespaceMismatch,
    /// Applied/committed state violates the no-snapshot rule or committed changed.
    IncompatibleCheckpoint,
    /// An expected committed point is genuinely absent.
    MissingEntry,
    /// The complete term/node/index identity differs.
    IdentityMismatch,
    /// SHA256 of the actual applied body differs from the saved checkpoint.
    BodyMismatch,
    /// The actual applied body's original stamp differs from saved control.
    StampMismatch,
    /// Applied clocks are below a known historical stamped entry.
    ClockMismatch,
    /// Saved/final membership is not the exact last committed membership.
    MembershipMismatch,
    /// No real supported driver exists for this closed body/profile.
    UnsupportedEntry,
    /// An actual driver/final observation disagrees with the expected control.
    ProgressMismatch,
    /// Existing durable log operational diagnosis.
    Log(RaftLogError),
    /// Existing same-guard state operational diagnosis.
    State(StateTransactionError),
    /// Existing complete Normal preparation/transaction diagnosis.
    Normal(RecordBatchTransactionError),
    /// Existing physical Claim preparation/transaction diagnosis.
    Claim(ClaimTransactionError),
    /// Existing complete core-policy preparation/transaction diagnosis.
    CorePolicy(CorePolicyTransactionError),
    /// Original public Policy.set preparation/transaction diagnosis.
    OriginalPolicy(OriginalPolicyTransactionError),
    /// Existing consumed workflow-value transaction diagnosis.
    Value(WorkflowValueTransactionError),
    /// Explicit identified governing-credit preparation/transaction diagnosis.
    Credit(wf_storage::transaction::CreditTransactionError),
    /// Header-only parsing failure.
    Codec(CodecError),
    /// Closed Blank/Membership parsing failure.
    Membership(RaftMembershipError),
}
impl fmt::Display for CommittedRecoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLimits => "invalid committed recovery limits",
            Self::Limit => "committed recovery limit",
            Self::NamespaceMismatch => "committed recovery namespace mismatch",
            Self::IncompatibleCheckpoint => "incompatible committed recovery checkpoint",
            Self::MissingEntry => "committed recovery point missing",
            Self::IdentityMismatch => "committed recovery log identity mismatch",
            Self::BodyMismatch => "committed recovery body digest mismatch",
            Self::StampMismatch => "committed recovery original stamp mismatch",
            Self::ClockMismatch => "committed recovery historical clock mismatch",
            Self::MembershipMismatch => "committed recovery membership mismatch",
            Self::UnsupportedEntry => "unsupported committed recovery entry",
            Self::ProgressMismatch => "committed recovery progress mismatch",
            Self::Log(error) => return fmt::Display::fmt(error, f),
            Self::State(error) => return fmt::Display::fmt(error, f),
            Self::Normal(error) => return fmt::Display::fmt(error, f),
            Self::Claim(error) => return fmt::Display::fmt(error, f),
            Self::CorePolicy(error) => return fmt::Display::fmt(error, f),
            Self::OriginalPolicy(error) => return fmt::Display::fmt(error, f),
            Self::Value(error) => return fmt::Display::fmt(error, f),
            Self::Credit(error) => return fmt::Display::fmt(error, f),
            Self::Codec(error) => return fmt::Display::fmt(error, f),
            Self::Membership(error) => return fmt::Display::fmt(error, f),
        })
    }
}
impl std::error::Error for CommittedRecoveryError {}
