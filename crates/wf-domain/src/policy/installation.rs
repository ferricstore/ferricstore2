//! Source generation/CAS install decisions and actual committed policy bytes.

use std::fmt;

use crate::retention::RetentionError;
use crate::value::DomainError;

use super::definition::{PolicyPatch, PolicySnapshot};

/// Source internal generation ceiling, independent of customer version.
pub const MAX_POLICY_GENERATION: u64 = 9_007_199_254_740_991;
/// Source normalized policy encoded snapshot maximum.
pub const MAX_POLICY_SNAPSHOT_BYTES: usize = 256 * 1_024;
/// Source encoded generation envelope has separate overhead allowance.
pub const MAX_ENCODED_POLICY_BYTES: usize = MAX_POLICY_SNAPSHOT_BYTES + 1_024;
/// Source aggregate snapshot occurrence maximum across a prepared batch.
pub const MAX_POLICY_BATCH_BYTES: usize = 4 * 1_024 * 1_024;

/// Immutable count/work policy bound, separate from actual source encoded bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PolicyLimits {
    max_states: usize,
}

impl Default for PolicyLimits {
    fn default() -> Self {
        Self { max_states: 16_384 }
    }
}
impl PolicyLimits {
    /// Configures bounded state work; every replica must use the same value.
    ///
    /// # Errors
    /// Rejects zero or a count above the smallest-byte source snapshot ceiling.
    pub const fn new(max_states: usize) -> Result<Self, PolicyError> {
        if max_states == 0 || max_states > MAX_POLICY_SNAPSHOT_BYTES {
            Err(PolicyError::StateBudget)
        } else {
            Ok(Self { max_states })
        }
    }
    /// Reads maximum unique policy states processed by this profile.
    #[must_use]
    pub const fn max_states(self) -> usize {
        self.max_states
    }
}

/// Source default low-level patch and DSL full replacement are distinct.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallMode {
    /// Preserve unspecified stored fields; merge supplied nested maps.
    Patch,
    /// Discard unspecified stored fields/state policies.
    Replace,
}

/// Supported install command, revalidated at ordered apply after preparation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallCommand {
    /// Fixed policy type, checked against any stored/prepared value.
    pub workflow_type: Vec<u8>,
    /// Optional CAS against stored generation, before migration high-water allocation.
    pub expected_generation: Option<u64>,
    /// Explicit replacement/recursive patch behavior.
    pub mode: InstallMode,
    /// Normalized supported caller fields, never serialized Rust layout.
    pub patch: PolicyPatch,
}
impl InstallCommand {
    /// Builds source low-level set with patch as the default.
    #[must_use]
    pub const fn low_level(workflow_type: Vec<u8>, patch: PolicyPatch) -> Self {
        Self {
            workflow_type,
            expected_generation: None,
            mode: InstallMode::Patch,
            patch,
        }
    }
    /// Admits borrowed caller policy fields before any candidate/definition copy.
    ///
    /// # Errors
    /// Returns exact supported source fields or immutable count/raw-byte rejection.
    pub fn admit(&self, limits: PolicyLimits) -> Result<(), PolicyError> {
        super::install_validation::validate_patch(self, limits)
    }
}

/// Deterministic candidate, encoded outside the domain before commitment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyCandidate {
    pub(super) generation: u64,
    pub(super) policy: PolicySnapshot,
}

impl PolicyCandidate {
    /// Borrows normalized final value for the private outer policy codec.
    #[must_use]
    pub const fn policy(&self) -> &PolicySnapshot {
        &self.policy
    }
    /// Reads generation the committed decision must revalidate.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    /// Binds actual independently verified snapshot/envelope bytes to this value.
    ///
    /// The outer codec must prove both decode exactly to this candidate; no client
    /// length/digest/authority flag can substitute. Domain admission uses actual
    /// lengths. Source ETF/hash preparation and durable write are not performed.
    ///
    /// # Errors
    /// Rejects final normalized/encoded sizes or malformed supported fields.
    pub fn bind_bytes(
        self,
        snapshot_bytes: &[u8],
        encoded: Vec<u8>,
        limits: PolicyLimits,
    ) -> Result<PolicyValue, PolicyError> {
        if snapshot_bytes.len() > MAX_POLICY_SNAPSHOT_BYTES {
            return Err(PolicyError::SnapshotTooLarge);
        }
        if encoded.len() > MAX_ENCODED_POLICY_BYTES {
            return Err(PolicyError::EncodedPolicyTooLarge);
        }
        super::install_validation::validate_snapshot(&self.policy, limits)?;
        Ok(PolicyValue {
            candidate: self,
            encoded,
        })
    }
    /// Binds an actual in-bounds snapshot span, moving both owners without a clone.
    ///
    /// The private outer codec must derive the range and verify these bytes decode
    /// exactly to this candidate. Length/value admission does not certify content,
    /// generation provenance, installation or durability.
    ///
    /// # Errors
    /// Rejects invalid ranges, actual byte caps or invalid normalized fields.
    pub fn bind_encoded_snapshot(
        self,
        range: std::ops::Range<usize>,
        encoded: Vec<u8>,
        limits: PolicyLimits,
    ) -> Result<PolicyValue, PolicyError> {
        let snapshot = encoded.get(range).ok_or(PolicyError::InvalidAuthority)?;
        if snapshot.len() > MAX_POLICY_SNAPSHOT_BYTES {
            return Err(PolicyError::SnapshotTooLarge);
        }
        if encoded.len() > MAX_ENCODED_POLICY_BYTES {
            return Err(PolicyError::EncodedPolicyTooLarge);
        }
        super::install_validation::validate_snapshot(&self.policy, limits)?;
        Ok(PolicyValue {
            candidate: self,
            encoded,
        })
    }
}

/// Immutable committed/authoritative policy value; construction is not storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyValue {
    candidate: PolicyCandidate,
    encoded: Vec<u8>,
}
impl PolicyValue {
    /// Reads internal authoritative generation, independently of customer version.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.candidate.generation
    }
    /// Borrows the complete represented normalized policy.
    #[must_use]
    pub const fn policy(&self) -> &PolicySnapshot {
        &self.candidate.policy
    }
    /// Borrows actual verified committed encoded bytes.
    #[must_use]
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
}

/// One pure proposed policy write with authoritative predecessor generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyInstallPlan<'a> {
    expected_generation: u64,
    value: &'a PolicyValue,
}
impl<'a> PolicyInstallPlan<'a> {
    /// Reads exact stored generation required by atomic persistence.
    #[must_use]
    pub const fn expected_generation(&self) -> u64 {
        self.expected_generation
    }
    /// Borrows immutable committed bytes/value until the adapter consumes the plan.
    #[must_use]
    pub const fn value(&self) -> &'a PolicyValue {
        self.value
    }
}

/// Source policy rejection or explicit operational apply failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyError {
    /// Existing/prepared authority is corrupt; never persist as business rejection.
    InvalidAuthority,
    /// Prepared value no longer matches current decision; apply cannot skip it.
    PreparedMismatch,
    /// Typed source field admission failed.
    Domain(DomainError),
    /// Public retention field admission failed.
    Retention(RetentionError),
    /// Customer version is empty/malformed.
    InvalidVersion,
    /// Type-only active duration must be positive and at most one year.
    InvalidMaxActive,
    /// Initial state must be a declared non-running logical state.
    InvalidInitialState,
    /// State mode must be omitted, parallel or fifo; explicit null is invalid.
    InvalidStateMode,
    /// Replacement state entries must be policy maps; null only deletes in patch.
    InvalidStatePolicy,
    /// Expected generation violates the exact source scalar bound.
    InvalidGeneration,
    /// Expected generation differs from stored generation.
    StaleGeneration,
    /// max(stored,migration)+1 cannot fit the source generation ceiling.
    GenerationExhausted,
    /// Policy state count/work exceeds the immutable profile.
    StateBudget,
    /// Final actual/typed snapshot exceeds the source encoded maximum.
    SnapshotTooLarge,
    /// Actual encoded generation envelope exceeds snapshot plus overhead.
    EncodedPolicyTooLarge,
    /// Summed snapshot bytes across a batch exceed the separate source maximum.
    SnapshotBatchTooLarge,
    /// Snapshot occurrences exceed the selected finite helper work profile.
    SnapshotBatchItems,
}
impl PolicyError {
    /// Identifies corruption/preparation failures that cannot be retained rejection.
    #[must_use]
    pub const fn is_apply_failure(self) -> bool {
        matches!(self, Self::InvalidAuthority | Self::PreparedMismatch)
    }
}
impl fmt::Display for PolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => error.fmt(formatter),
            Self::Retention(error) => error.fmt(formatter),
            _ => formatter.write_str(match self {
                Self::InvalidAuthority => "invalid authoritative flow policy",
                Self::PreparedMismatch => {
                    "committed policy preparation does not match current authority"
                }
                Self::InvalidVersion => {
                    "ERR flow policy version must be a non-empty string or non-negative integer"
                }
                Self::InvalidMaxActive => {
                    "ERR flow max_active_ms must be between 1 and 31536000000 or infinity"
                }
                Self::InvalidInitialState => "invalid declared workflow initial state",
                Self::InvalidStateMode => "ERR flow state mode must be parallel or fifo",
                Self::InvalidStatePolicy => "ERR flow state policy must be a map or keyword list",
                Self::InvalidGeneration => "ERR invalid flow policy generation",
                Self::StaleGeneration => "ERR stale flow policy generation",
                Self::GenerationExhausted => "ERR flow policy generation exhausted",
                Self::StateBudget => "workflow policy state budget exceeded",
                Self::SnapshotTooLarge => "ERR flow policy snapshot exceeds 262144 bytes",
                Self::EncodedPolicyTooLarge => "encoded flow policy exceeds 263168 bytes",
                Self::SnapshotBatchTooLarge => {
                    "ERR flow policy snapshot batch exceeds 4194304 bytes"
                }
                Self::SnapshotBatchItems => {
                    "workflow policy snapshot occurrence count exceeds 100000"
                }
                Self::Domain(_) | Self::Retention(_) => "invalid typed flow policy",
            }),
        }
    }
}
impl std::error::Error for PolicyError {}

/// Validates a borrowed stored policy against this type and selected profile.
///
/// This certifies no namespace, encoded content, installation or durable state.
///
/// # Errors
/// Maps invalid stored generations, values or budgets to `InvalidAuthority`.
pub fn validate_stored_policy(
    value: &PolicyValue,
    workflow_type: &[u8],
    limits: PolicyLimits,
) -> Result<(), PolicyError> {
    if value.generation() == 0
        || value.generation() > MAX_POLICY_GENERATION
        || value.policy().workflow_type() != workflow_type
        || value.encoded().len() > MAX_ENCODED_POLICY_BYTES
    {
        return Err(PolicyError::InvalidAuthority);
    }
    super::install_validation::validate_snapshot(value.policy(), limits)
        .map_err(|_| PolicyError::InvalidAuthority)
}

/// Prepares a bounded pure candidate using stored authority and migration high-water.
///
/// # Errors
/// Rejects source admission/CAS/exhaustion; invalid authority is operational.
pub fn prepare_install(
    existing: Option<&PolicyValue>,
    migration_high_water: u64,
    command: &InstallCommand,
    limits: PolicyLimits,
) -> Result<PolicyCandidate, PolicyError> {
    let stored_generation = existing.map_or(0, PolicyValue::generation);
    if stored_generation > MAX_POLICY_GENERATION || migration_high_water > MAX_POLICY_GENERATION {
        return Err(PolicyError::InvalidAuthority);
    }
    if let Some(expected) = command.expected_generation {
        if expected > MAX_POLICY_GENERATION {
            return Err(PolicyError::InvalidGeneration);
        }
        if expected != stored_generation {
            return Err(PolicyError::StaleGeneration);
        }
    }
    if let Some(existing) = existing {
        validate_stored_policy(existing, &command.workflow_type, limits)?;
    }
    super::install_validation::validate_patch(command, limits)?;
    let base = match (command.mode, existing) {
        (InstallMode::Patch, Some(existing)) => Some(existing.policy()),
        _ => None,
    };
    super::preview::validate_final(base, command, limits)?;
    let generation = stored_generation
        .max(migration_high_water)
        .checked_add(1)
        .filter(|generation| *generation <= MAX_POLICY_GENERATION)
        .ok_or(PolicyError::GenerationExhausted)?;
    let mut policy = match (command.mode, existing) {
        (InstallMode::Patch, Some(existing)) => existing.policy().clone(),
        _ => PolicySnapshot::empty(command.workflow_type.clone()),
    };
    super::patch::merge(&mut policy, &command.patch);
    super::install_validation::validate_snapshot(&policy, limits)?;
    Ok(PolicyCandidate { generation, policy })
}

/// Recomputes the decision at ordered apply and compares complete prepared value.
///
/// # Errors
/// Rejects invalid source inputs/guards or operational prepared/authority mismatch.
pub fn decide_install<'a>(
    existing: Option<&PolicyValue>,
    migration_high_water: u64,
    command: &InstallCommand,
    prepared: &'a PolicyValue,
    limits: PolicyLimits,
) -> Result<PolicyInstallPlan<'a>, PolicyError> {
    let candidate = prepare_install(existing, migration_high_water, command, limits)?;
    if candidate != prepared.candidate {
        return Err(PolicyError::PreparedMismatch);
    }
    Ok(PolicyInstallPlan {
        expected_generation: existing.map_or(0, PolicyValue::generation),
        value: prepared,
    })
}

/// Checks actual occurrence bytes across a prepared policy snapshot batch.
/// Count uses a selected helper work profile with the atomic ceiling's number;
/// the source byte rule can include multiple snapshots per command. Callers
/// must additionally enforce their lower configured item profile. Bytes alone
/// cannot certify occurrence work, even when supplied slices are empty.
///
/// # Errors
/// Rejects checked sum overflow or aggregate source batch-size excess.
pub fn validate_snapshot_batch(snapshots: &[&[u8]]) -> Result<(), PolicyError> {
    if snapshots.len() > crate::workflow::admission::MAX_BATCH_ITEMS {
        return Err(PolicyError::SnapshotBatchItems);
    }
    let bytes = snapshots.iter().try_fold(0_usize, |total, bytes| {
        total
            .checked_add(bytes.len())
            .ok_or(PolicyError::SnapshotBatchTooLarge)
    })?;
    if bytes > MAX_POLICY_BATCH_BYTES {
        Err(PolicyError::SnapshotBatchTooLarge)
    } else {
        Ok(())
    }
}
