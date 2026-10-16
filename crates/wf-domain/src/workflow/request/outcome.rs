//! Immutable original retained outcome owner and business-only semantic status.

use std::fmt;

use wf_types::identity::RequestId;
use wf_types::version::{LogIdentity, TimestampMillis};

use super::{ClientIntent, RequestScope};
use crate::claim::apply::ClaimRejection;
use crate::policy::complete::{
    CompletePolicyReplaceRejection, FlowPolicyInputRejection, OriginalFlowPolicyReplaceRejection,
};
use crate::policy::installation::PolicyError;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::BatchRejection;
use crate::workflow::indexed::IndexRejection;

/// Original durable semantic status; operational failure is unrepresentable here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetainedStatus {
    /// Original command was accepted, including source legacy no-op/empty claim.
    Accepted,
    /// Original deterministic rejection is replayed without current guard checks.
    Rejected(BatchRejection),
    /// Original validated policy business rejection, without operational failures.
    PolicyRejected(PolicyRejection),
    /// Original reached complete-Replace content rejection, including captured context.
    CompletePolicyReplaceRejected(CompletePolicyReplaceRejection),
    /// Original reached whole-flow replacement diagnosis, without operational failures.
    OriginalFlowPolicyReplaceRejected(OriginalFlowPolicyReplaceRejection),
    /// Original reached public set diagnosis, with its captured business context.
    OriginalPolicySetRejected(FlowPolicyInputRejection),
    /// Original canonical index guard rejection.
    IndexedRejected(IndexRejection),
    /// Original business-only claim rejection, without operational authority errors.
    ClaimRejected(ClaimRejection),
}

/// Policy error admitted for retention only when it is a business decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PolicyRejection(PolicyError);
impl PolicyRejection {
    /// Validates that authority/preparation failures cannot become durable status.
    ///
    /// # Errors
    /// Rejects operational policy failures.
    pub const fn new(error: PolicyError) -> Result<Self, RequestValueError> {
        if error.is_apply_failure() {
            Err(RequestValueError::OperationalStatus)
        } else {
            Ok(Self(error))
        }
    }
    /// Reads the original deterministic policy error.
    #[must_use]
    pub const fn error(self) -> PolicyError {
        self.0
    }
}

/// Immutable original metadata bound to retained response bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedMetadata {
    /// Original tenant/group/recovery namespace.
    pub scope: RequestScope,
    /// Stable caller request identity.
    pub request: RequestId,
    /// Original canonical client intent.
    pub intent: ClientIntent,
    /// Original full log identity, distinct from duplicate processing position.
    pub original_position: LogIdentity,
    /// Original execution time.
    pub original_time: TimestampMillis,
    /// Immutable original retention deadline.
    pub expires_at: TimestampMillis,
    /// Original accepted or deterministic-rejected status.
    pub status: RetainedStatus,
}

/// Validated original response owner, supplied by durable authoritative reads.
///
/// Holds immutable versioned actual bytes so replay never serializes changed
/// current records. Future apply persists this with records/indexes/history and
/// checkpoint; constructing it does not establish that persistence happened.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedOutcome {
    pub(super) scope: RequestScope,
    pub(super) request: RequestId,
    pub(super) intent: ClientIntent,
    pub(super) original_position: LogIdentity,
    pub(super) original_time: TimestampMillis,
    pub(super) expires_at: TimestampMillis,
    status: RetainedStatus,
    response_version: u16,
    pub(super) response: Vec<u8>,
}

impl RetainedOutcome {
    /// Admits actual retained bytes and the immutable original retention window.
    ///
    /// # Errors
    /// Rejects unsupported format, oversized actual response or invalid expiry.
    pub fn new(
        metadata: RetainedMetadata,
        response_version: u16,
        response: Vec<u8>,
        limits: AdmissionLimits,
    ) -> Result<Self, RequestValueError> {
        let RetainedMetadata {
            scope,
            request,
            intent,
            original_position,
            original_time,
            expires_at,
            status,
        } = metadata;
        if response_version != 1
            && !(response_version == 2
                && intent.version() == 1
                && matches!(
                    status,
                    RetainedStatus::Accepted | RetainedStatus::IndexedRejected(_)
                ))
        {
            return Err(RequestValueError::UnsupportedVersion);
        }
        if expires_at <= original_time {
            return Err(RequestValueError::InvalidExpiry);
        }
        if response.len() > limits.decision_bytes() {
            return Err(RequestValueError::ResponseTooLarge);
        }
        Ok(Self {
            scope,
            request,
            intent,
            original_position,
            original_time,
            expires_at,
            status,
            response_version,
            response,
        })
    }
    /// Borrows the original retained namespace without copying its identities.
    #[must_use]
    pub const fn scope(&self) -> &RequestScope {
        &self.scope
    }
    /// Borrows the original request identity.
    #[must_use]
    pub const fn request(&self) -> &RequestId {
        &self.request
    }
    /// Reads the original client intent, independently of log position.
    #[must_use]
    pub const fn intent(&self) -> ClientIntent {
        self.intent
    }
    /// Borrows original response bytes; empty bytes remain present.
    #[must_use]
    pub fn response(&self) -> &[u8] {
        &self.response
    }
    /// Reads immutable retained response format version.
    #[must_use]
    pub const fn response_version(&self) -> u16 {
        self.response_version
    }
    /// Reads original full log identity, distinct from current duplicate position.
    #[must_use]
    pub const fn original_position(&self) -> LogIdentity {
        self.original_position
    }
    /// Reads original execution time, never renewed by duplicate processing.
    #[must_use]
    pub const fn original_time(&self) -> TimestampMillis {
        self.original_time
    }
    /// Reads original retention deadline.
    #[must_use]
    pub const fn expires_at(&self) -> TimestampMillis {
        self.expires_at
    }
    /// Reads original semantic status.
    #[must_use]
    pub const fn status(&self) -> RetainedStatus {
        self.status
    }
}

/// Primitive retained/intent construction error; no outcome persistence occurs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestValueError {
    /// Intent or retained response format is unsupported.
    UnsupportedVersion,
    /// Retention expiry must be later than original execution time.
    InvalidExpiry,
    /// Actual retained response exceeds the immutable aggregate bound.
    ResponseTooLarge,
    /// Operational apply failure must not be retained as business status.
    OperationalStatus,
}

impl fmt::Display for RequestValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedVersion => "unsupported workflow request format version",
            Self::InvalidExpiry => "invalid original workflow outcome retention deadline",
            Self::ResponseTooLarge => "retained workflow response byte budget exceeded",
            Self::OperationalStatus => "operational failure is not a retained workflow outcome",
        })
    }
}
impl std::error::Error for RequestValueError {}
