//! Immutable framing budgets and exact prepared-byte ownership.
use wf_domain::{
    policy::installation::PolicyLimits,
    workflow::{admission::AdmissionLimits, control::PreparedDigest},
};

/// Immutable byte/count budgets shared by encoder, decoder and ordered apply.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CodecLimits {
    pub(super) admission: AdmissionLimits,
    pub(super) policy: Option<PolicyLimits>,
}

impl CodecLimits {
    /// Uses validated immutable admission values; no local resource input is read.
    #[must_use]
    pub const fn new(admission: AdmissionLimits) -> Self {
        Self {
            admission,
            policy: None,
        }
    }
    /// Selects exact validated policy work bounds without changing payload limits.
    #[must_use]
    pub const fn with_policy_limits(admission: AdmissionLimits, policy: PolicyLimits) -> Self {
        Self {
            admission,
            policy: Some(policy),
        }
    }
    /// Resolves the immutable v1 policy profile, including its frozen default.
    #[must_use]
    pub fn policy_limits(self) -> PolicyLimits {
        self.policy.unwrap_or_default()
    }
    /// Reads the replicated profile used for all actual encoded data.
    #[must_use]
    pub const fn admission(self) -> AdmissionLimits {
        self.admission
    }
}

/// Exact private prepared bytes with SHA256 identity; no own digest field exists.
pub struct PreparedBytes {
    pub(super) bytes: Vec<u8>,
    pub(super) digest: PreparedDigest,
}

impl PreparedBytes {
    /// Transfers the original wire owner without copying or changing capacity.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
    /// Borrows exact prepared bytes, excluding separately assigned log ID.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Reads prepared identity independently from client intent and log identity.
    #[must_use]
    pub const fn digest(&self) -> PreparedDigest {
        self.digest
    }
}
