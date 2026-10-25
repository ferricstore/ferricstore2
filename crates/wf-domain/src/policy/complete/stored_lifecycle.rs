//! Creation capture over a retained complete or original stored-policy donor.

use super::{
    CompletePolicyEncodingError, PolicySchemaCharge, PolicySchemaLimits, StoredPolicyValue,
};
use crate::workflow::active::{
    ActiveValueError, CapturedLifecycle, LifecycleDefaults, LifecycleOverrides,
};

/// Ordinary creation inputs; these values certify no policy or storage authority.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct StoredLifecycleCaptureInput<'input> {
    /// Workflow type bound to the actual retained policy donor.
    pub workflow_type: &'input [u8],
    /// Initial state whose lifecycle layer precedes explicit command overrides.
    pub initial_state: &'input [u8],
    /// Presence-preserving command lifecycle fields.
    pub overrides: LifecycleOverrides,
    /// Trusted configured defaults and clamping bounds.
    pub defaults: LifecycleDefaults,
}

/// Distinct stored-policy operational and creation-value refusal origins.
#[derive(Debug)]
pub enum StoredLifecycleCaptureError {
    /// Stored backing, profile, schema work or temporary-owner refusal.
    Policy(CompletePolicyEncodingError),
    /// Invalid reached scalar defaults or command overrides.
    Capture(ActiveValueError),
}

/// Resolves creation scalars from a retained full policy under one work account.
///
/// This callable baseline deliberately rejects every input until the full donor
/// bridge has genuine failing-test evidence. It performs no decode or capture.
///
/// # Errors
/// Currently returns the operational unsupported-policy-profile diagnosis.
pub fn capture_stored_creation_lifecycle(
    _policy: Option<&StoredPolicyValue>,
    _input: StoredLifecycleCaptureInput<'_>,
    _schema: PolicySchemaLimits,
    _charge: &mut PolicySchemaCharge,
    _maximum_temporary_bytes: usize,
) -> Result<CapturedLifecycle, StoredLifecycleCaptureError> {
    Err(StoredLifecycleCaptureError::Policy(
        CompletePolicyEncodingError::UnsupportedPolicyProfile,
    ))
}
