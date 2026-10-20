//! Ownership-preserving outer Complete Replace and core Patch request admission.

use std::fmt;

use crate::workflow::admission::AdmissionLimits;

use super::{
    CompletePolicyEncodingError, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};

mod caller;
mod core_patch_frame;
pub(in crate::policy::complete) mod flow_frame;
mod model;
pub(in crate::policy::complete) mod public_set_frame;
mod replace_frame;

pub use public_set_frame::{
    OriginalPolicySetFrameBytes, OriginalPolicySetFrameHeader, OriginalPolicySetFrameInput,
    ScannedOriginalPolicySetFrame, prepare_original_policy_set_frame_bytes,
    restore_original_policy_set_frame_header, restore_original_policy_set_point_frame_header,
    scan_original_policy_set_frame,
};

pub use flow_frame::{
    OriginalFlowPolicyReplaceFrameHeader, OriginalFlowPolicyReplaceInput,
    ScannedOriginalFlowPolicyReplaceFrame, prepare_original_flow_policy_replace_frame_bytes,
    restore_original_flow_policy_replace_frame_header, scan_original_flow_policy_replace_frame,
};

pub use replace_frame::{
    CompletePolicyReplaceFrameHeader, ScannedCompletePolicyReplaceFrame,
    decode_complete_policy_replace_frame_input, restore_complete_policy_replace_frame_header,
    scan_complete_policy_replace_frame,
};

pub use core_patch_frame::{
    CompleteCorePolicyPatchFrameHeader, ScannedCorePatchFrame,
    complete_core_policy_patch_frame_decode_bound, decode_complete_core_policy_patch_frame_input,
    restore_complete_core_policy_patch_frame_header, scan_complete_core_policy_patch_frame,
};

pub use model::{
    CompleteCorePolicyPatchCaller, CompleteCorePolicyPatchRequestInput,
    CompletePolicyReplaceCaller, CompletePolicyReplaceInput,
};

/// Operational refusal of outer request preparation, before ordered installation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletePolicyRequestError {
    /// Complete schema or private canonical encoding admission failed.
    Encoding(CompletePolicyEncodingError),
    /// The complete derived request namespace exceeds its fixed bound.
    ScopeLimit,
    /// A supplied intent identity differs from its independently recomputed bytes.
    IntentMismatch,
}

impl fmt::Display for CompletePolicyRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encoding(error) => fmt::Display::fmt(error, formatter),
            Self::ScopeLimit => formatter.write_str("complete policy request scope limit exceeded"),
            Self::IntentMismatch => formatter.write_str("complete policy request intent mismatch"),
        }
    }
}

impl std::error::Error for CompletePolicyRequestError {}

impl From<CompletePolicyEncodingError> for CompletePolicyRequestError {
    fn from(error: CompletePolicyEncodingError) -> Self {
        Self::Encoding(error)
    }
}

impl From<PolicySchemaError> for CompletePolicyRequestError {
    fn from(error: PolicySchemaError) -> Self {
        Self::Encoding(CompletePolicyEncodingError::Schema(error))
    }
}

/// Bounds caller intent work on the same continuous schema account.
///
/// The selected profiles are retained without granting installation authority.
/// Caller construction and refused-owner destruction remain outside logical work.
///
/// # Errors
/// Preserves profile, work, overflow, byte, count, and scope refusals.
pub fn prepare_complete_policy_replace_caller(
    input: CompletePolicyReplaceInput,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyReplaceCaller, CompletePolicyRequestError> {
    caller::prepare_caller(input, limits, admission, charge)
}

/// Bounds original partial-command intent work on the same continuous account.
///
/// Retains the supplied owners and profiles for later current-policy remerge.
///
/// # Errors
/// Preserves profile, work, overflow, byte, count, and scope refusals.
pub fn prepare_complete_core_policy_patch_caller(
    input: CompleteCorePolicyPatchRequestInput,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompleteCorePolicyPatchCaller, CompletePolicyRequestError> {
    caller::prepare_core_patch_caller(input, limits, admission, charge)
}

/// Bounds original core-Patch frame preparation on one continuous schema account.
///
/// Returns the original caller and exact intent/frame lengths as mathematical
/// content only. The closed ports entry freshly prepares raw input before actual
/// SHA and bounded output; these values supply no charge or installation authority.
///
/// # Errors
/// Preserves profile, work, scope, original-intent and complete-frame refusals.
pub fn prepare_complete_core_policy_patch_frame_caller(
    input: CompleteCorePolicyPatchRequestInput,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(CompleteCorePolicyPatchCaller, usize, usize), CompletePolicyRequestError> {
    caller::prepare_core_patch_frame_caller(input, limits, admission, charge)
}

/// Bounds original Complete Replace frame preparation on one continuous account.
///
/// Returns immutable caller content and mathematical original/frame lengths.
/// These values carry no reusable payment or installation authority.
///
/// # Errors
/// Preserves profile, work, scope, canonical-list and complete-frame refusals.
pub fn prepare_complete_policy_replace_frame_caller(
    input: CompletePolicyReplaceInput,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(CompletePolicyReplaceCaller, usize, usize), CompletePolicyRequestError> {
    caller::prepare_replace_frame_caller(input, limits, admission, charge)
}
