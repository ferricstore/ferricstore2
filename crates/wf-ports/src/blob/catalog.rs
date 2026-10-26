//! Owned workflow value observations, without publication authority.

use wf_domain::workflow::active::{ActiveValueRefs, CapturedLifecycle};
use wf_domain::workflow::control::PreparedDigest;
use wf_types::version::TimestampMillis;

/// Captured birth provenance and the current effective terminal lifetime.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct WorkflowValueLifecycle {
    /// Validated state-entry sequence identifying the birth.
    pub incarnation: u128,
    /// Immutable creation-time lifecycle capture.
    pub birth: CapturedLifecycle,
    /// Current terminal retention; absence means no expiry.
    pub effective_ttl_ms: Option<u64>,
    /// Resolved record terminal deadline; existing values expire independently.
    pub terminal_deadline: Option<TimestampMillis>,
    /// Configured defaults used for this birth.
    pub configuration_digest: PreparedDigest,
    /// Selected retained policy backing profile.
    pub policy_profile: u8,
    /// Selected installed policy generation.
    pub policy_generation: u64,
    /// Exact selected retained policy backing digest.
    pub policy_digest: PreparedDigest,
}

/// One current or historical binding and its captured lifecycle provenance.
pub struct WorkflowValueBinding {
    /// Original public payload, result and error references.
    pub refs: ActiveValueRefs,
    /// Birth provenance and current effective lifetime.
    pub lifecycle: WorkflowValueLifecycle,
}

/// Distinguishes unavailable legacy provenance from a known missing binding.
pub enum WorkflowValueBindingKnowledge {
    /// This store's legacy profile has no value binding provenance.
    LegacyUnavailable,
    /// Value-capable authority observed either the binding or its absence.
    Known(Option<Box<WorkflowValueBinding>>),
}

/// Per-reference read refusal, separate from a known missing value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkflowValueReadFailure {
    /// The underlying value could not be read.
    ReadFailed,
    /// The value is unavailable under the supported storage capability.
    Unavailable,
    /// HLC and wall drift makes expiry unsafe to classify as missing.
    UnsafeDrift,
    /// The selected value profile is not supported.
    UnsupportedProfile,
}

/// Ordered read slots share one owner for each distinct resolved value.
pub struct WorkflowValueReadBatch {
    /// Distinct owned stored FSV2 bytes used by successful slots.
    pub values: Vec<Vec<u8>>,
    /// Original request order; absence means a known missing value.
    pub slots: Vec<Result<Option<usize>, WorkflowValueReadFailure>>,
}

/// Explicit logical ceilings, separate from native page cache and RSS.
pub struct WorkflowValueReadLimits {
    /// Maximum number of input references.
    pub maximum_references: usize,
    /// Maximum cumulative admitted stored bytes.
    pub maximum_stored_bytes: usize,
    /// Maximum cumulative charged read work.
    pub maximum_work: usize,
    /// Maximum simultaneous logical temporary ownership.
    pub maximum_temporary_bytes: usize,
}
