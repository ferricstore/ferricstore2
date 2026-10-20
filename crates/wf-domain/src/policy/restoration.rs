//! Validated normalized policy values without encoding or installed authority.

use std::collections::BTreeMap;

use crate::retention::RetentionOverride;

use super::RetryOverride;
use super::definition::{PolicySnapshot, PolicyVersion, StatePolicy};
use super::installation::{MAX_POLICY_GENERATION, PolicyCandidate, PolicyError, PolicyLimits};

/// Owned normalized policy fields for explicit pure value restoration.
///
/// These fields carry no scope, source/disk provenance, encoded binding,
/// installation, migration high-water or apply permission. A private decoder
/// must validate borrowed bytes/counts before allocating these owned fields.
/// Logical value limits do not bound supplied Vec spare capacity or guarantee
/// recoverable allocation. Construction preserves normalized Option semantics.
pub struct PolicySnapshotData {
    /// Exact nonempty opaque workflow type.
    pub workflow_type: Vec<u8>,
    /// Optional customer version, independent of internal generation.
    pub version: Option<PolicyVersion>,
    /// Optional positive type active duration; absence means infinity.
    pub max_active_ms: Option<u64>,
    /// Normalized type retry overrides, preserving absent and explicit fields.
    pub retry: RetryOverride,
    /// Normalized public type retention overrides, without resolved defaults.
    pub retention: RetentionOverride,
    /// Unique binary-sorted normalized state fields, moved without a map clone.
    pub states: BTreeMap<Vec<u8>, StatePolicy>,
}

/// Validates an explicit installed-generation value without allocating a new one.
///
/// Fields move into a candidate unchanged. This provides no byte binding,
/// installation or durability proof and does not simulate policy allocation.
///
/// # Errors
/// Rejects generation outside `1..=MAX_POLICY_GENERATION` or invalid/budgeted fields.
pub fn restore_candidate(
    generation: u64,
    data: PolicySnapshotData,
    limits: PolicyLimits,
) -> Result<PolicyCandidate, PolicyError> {
    if generation == 0 || generation > MAX_POLICY_GENERATION {
        return Err(PolicyError::InvalidGeneration);
    }
    let policy = restore_policy_snapshot(data, limits)?;
    Ok(PolicyCandidate { generation, policy })
}

/// Restores generation-free mode/content facts without installed authority.
/// # Errors
/// Preserves the existing snapshot field and owner budget validation.
pub fn restore_policy_snapshot(
    data: PolicySnapshotData,
    limits: PolicyLimits,
) -> Result<PolicySnapshot, PolicyError> {
    let PolicySnapshotData {
        workflow_type,
        version,
        max_active_ms,
        retry,
        retention,
        states,
    } = data;
    let policy = PolicySnapshot {
        workflow_type,
        version,
        max_active_ms,
        retry,
        retention,
        states,
    };
    super::install_validation::validate_snapshot(&policy, limits)?;
    Ok(policy)
}

impl PolicyCandidate {
    /// Moves mathematical snapshot facts without installing or certifying them.
    #[must_use]
    pub fn into_policy(self) -> PolicySnapshot {
        self.policy
    }
}
