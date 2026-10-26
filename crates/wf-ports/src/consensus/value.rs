//! Explicit prepared value inputs, without publication or policy authority.

use sha2::{Digest, Sha256};
use wf_domain::workflow::active::{LifecycleDefaults, LifecycleOverrides};
use wf_domain::workflow::control::PreparedDigest;
use wf_domain::workflow::request::RequestScope;
use wf_types::identity::RequestId;
use wf_types::version::{CommittedTime, TimestampMillis};

use super::PreparedOperation;
use crate::log::CodecError;

/// Owned caller operation and server preparation inputs for one value batch.
pub struct PreparedWorkflowValueBatch {
    /// Bound tenant, group and recovery scope.
    pub scope: RequestScope,
    /// Stable request identity, separate from caller intent.
    pub request: RequestId,
    /// Trusted committed preparation time.
    pub stamp: CommittedTime,
    /// Contained reducer profile.
    pub profile: u8,
    /// Explicit source marker presence and value.
    pub policy_references_captured: Option<bool>,
    /// Configured lifecycle defaults captured by preparation.
    pub defaults: WorkflowValueDefaults,
    /// Ordered witnesses for the actual retained policy backing.
    pub policies: Vec<WorkflowValuePolicyWitness>,
    /// Existing canonical operation and caller evidence owner.
    pub operation: PreparedOperation,
    /// Source presence and guard inputs in operation order.
    pub inputs: Vec<WorkflowValueItemInput>,
}

/// Configured lifecycle values and their content identity.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct WorkflowValueDefaults {
    /// Digest of the versioned canonical configured defaults.
    pub configuration_digest: PreparedDigest,
    /// Trusted configured durations and history caps.
    pub lifecycle: LifecycleDefaults,
}

impl WorkflowValueDefaults {
    /// Encodes validated defaults and their exact configuration identity.
    ///
    /// # Errors
    /// Rejects invalid scalar defaults or an inconsistent supplied digest.
    pub fn to_bytes(self) -> Result<[u8; 60], CodecError> {
        self.lifecycle
            .validate()
            .map_err(|_| CodecError::Malformed)?;
        let mut bytes = [0; 60];
        bytes[0..8].copy_from_slice(&self.lifecycle.ttl_ms.to_be_bytes());
        bytes[8..12].copy_from_slice(&self.lifecycle.history_hot_max_events.to_be_bytes());
        bytes[12..16].copy_from_slice(&self.lifecycle.history_max_events.to_be_bytes());
        bytes[16..24].copy_from_slice(&self.lifecycle.max_history_hot_max_events.to_be_bytes());
        bytes[24..28].copy_from_slice(&self.lifecycle.max_history_max_events.to_be_bytes());
        let mut hash = Sha256::new();
        hash.update(b"wf-workflow-value-defaults-v1");
        hash.update(&bytes[..28]);
        let actual: [u8; 32] = hash.finalize().into();
        if &actual != self.configuration_digest.as_bytes() {
            return Err(CodecError::Malformed);
        }
        bytes[28..].copy_from_slice(&actual);
        Ok(bytes)
    }

    /// Restores the same fixed grammar without repairing a claimed identity.
    ///
    /// # Errors
    /// Rejects invalid scalar defaults or an inconsistent configuration digest.
    pub fn from_bytes(bytes: [u8; 60]) -> Result<Self, CodecError> {
        let value = Self {
            configuration_digest: PreparedDigest::new(
                bytes[28..60]
                    .try_into()
                    .map_err(|_| CodecError::Malformed)?,
            ),
            lifecycle: LifecycleDefaults {
                ttl_ms: u64::from_be_bytes(
                    bytes[0..8].try_into().map_err(|_| CodecError::Malformed)?,
                ),
                history_hot_max_events: u32::from_be_bytes(
                    bytes[8..12].try_into().map_err(|_| CodecError::Malformed)?,
                ),
                history_max_events: u32::from_be_bytes(
                    bytes[12..16]
                        .try_into()
                        .map_err(|_| CodecError::Malformed)?,
                ),
                max_history_hot_max_events: u64::from_be_bytes(
                    bytes[16..24]
                        .try_into()
                        .map_err(|_| CodecError::Malformed)?,
                ),
                max_history_max_events: u32::from_be_bytes(
                    bytes[24..28]
                        .try_into()
                        .map_err(|_| CodecError::Malformed)?,
                ),
            },
        };
        value.to_bytes()?;
        Ok(value)
    }
}

/// Exact policy backing observed during preparation; apply revalidates it.
pub struct WorkflowValuePolicyWitness {
    /// Exact workflow type bytes.
    pub workflow_type: Vec<u8>,
    /// Canonical absent, complete or original backing profile.
    pub profile: u8,
    /// Observed installed generation.
    pub generation: u64,
    /// Digest of the exact retained encoded backing.
    pub digest: PreparedDigest,
}

/// Captured target condition, distinct from an omitted caller condition.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum WorkflowValueTargetGuard {
    /// Preparation did not capture target presence.
    Unspecified,
    /// Preparation observed no current target.
    Absent,
    /// Preparation observed this birth of the selected policy's type.
    Present {
        /// Validated state-entry sequence identifying the birth.
        incarnation: u128,
    },
}

/// Per-source-item inputs that preserve optionality before donor decoding ends.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct WorkflowValueItemInput {
    /// Original position in the ordered source operation.
    pub source_index: u32,
    /// Selected witness index within the prepared batch.
    pub policy_index: u32,
    /// Observed target guard, revalidated against current authority.
    pub target: WorkflowValueTargetGuard,
    /// Birth overrides, including explicit absence.
    pub birth: Option<LifecycleOverrides>,
    /// Signed terminal override; nonpositive means no expiry.
    pub terminal_ttl_ms: Option<i64>,
    /// Optional caller-requested execution time.
    pub requested_at: Option<TimestampMillis>,
}
