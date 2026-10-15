//! Complete normalized policy values and borrowed known governance content.
//!
//! These values preserve normalized presence and owners. They do not install a
//! policy or certify encoding, source ETF sizes/hashes, namespaces or services.

use std::collections::BTreeMap;

use super::RetryOverride;
use super::definition::PolicyVersion;
use crate::retention::RetentionOverride;

mod admission;

pub(crate) use admission::charge_schema;
mod encoding;
mod error;
mod flow_input;
mod indexed_input;
mod installation;
mod model;
mod original_entry;
mod original_reply;
mod original_retry_keys;
mod outer;
mod patch;
mod presentation;
mod public_set;
mod replace_content;
mod replace_request;
mod request;
mod resolution;
mod restoration;
mod retention_input;
mod retry_input;
mod rules;
mod validation;

pub use admission::{PolicySchemaCharge, PolicySchemaLimits};
pub use encoding::{
    CompletePolicyEncodingError, decode_complete_policy_value_v2, prepare_complete_policy_value_v2,
    verify_complete_policy_value_v2,
};
pub use error::{CircuitTrackingFailure, GovernanceField, GovernanceRuleKind, PolicySchemaError};
pub use flow_input::{
    FlowPolicyInputRejection, GovernancePolicyInputRejection, RawFlowPolicyValue,
    encode_original_complete_policy_input_rejection_body, normalize_original_complete_policy_input,
};
pub use indexed_input::{
    PolicyIndexedInputRejection, RawPolicyIndexedValue, normalize_complete_policy_indexed_input,
};
pub use installation::{
    CompletePolicyInstallPlan, PreparedCompletePolicyReplace, decide_complete_policy_replace,
    prepare_complete_policy_replace,
};
pub use model::{
    ApprovalsData, AuditData, BudgetData, CircuitData, CompleteStatePolicyData, DenialAuditMode,
    EffectsData, GovernanceData, GovernanceMode, LimitData,
};
pub use original_entry::{
    OriginalEtfLocalFun, OriginalEtfSafety, OriginalPolicyEntry, OriginalPolicyReadError,
    OriginalPolicyReadValue, decode_original_complete_policy_entry,
    prepare_original_complete_policy_reference_bytes, read_original_complete_policy_entry,
};
pub use original_reply::{
    OriginalPolicyReply, OriginalPolicyReplyContext, OriginalPolicyReplyError,
    OriginalPolicyReplyMapEntry, OriginalPolicyReplyMapIter, OriginalPolicyReplyRejection,
    OriginalPolicyReplyValue, present_original_policy_get_reply, present_original_policy_set_reply,
};
pub use original_retry_keys::{
    OriginalPolicyMapKey, OriginalPolicyMapValue, original_complete_policy_uses_max_attempts,
};
pub use outer::{
    CompleteCorePolicyPatchCaller, CompleteCorePolicyPatchFrameHeader,
    CompleteCorePolicyPatchRequestInput, CompletePolicyReplaceCaller, CompletePolicyReplaceInput,
    CompletePolicyRequestError, ScannedCorePatchFrame,
    decode_complete_core_policy_patch_frame_input, prepare_complete_core_policy_patch_caller,
    prepare_complete_core_policy_patch_frame_caller, prepare_complete_policy_replace_caller,
    restore_complete_core_policy_patch_frame_header, scan_complete_core_policy_patch_frame,
};
pub use outer::{
    CompletePolicyReplaceFrameHeader, ScannedCompletePolicyReplaceFrame,
    decode_complete_policy_replace_frame_input, prepare_complete_policy_replace_frame_caller,
    restore_complete_policy_replace_frame_header, scan_complete_policy_replace_frame,
};
pub use outer::{
    OriginalFlowPolicyReplaceFrameHeader, OriginalFlowPolicyReplaceInput,
    ScannedOriginalFlowPolicyReplaceFrame, prepare_original_flow_policy_replace_frame_bytes,
    restore_original_flow_policy_replace_frame_header, scan_original_flow_policy_replace_frame,
};
pub use outer::{
    OriginalPolicySetFrameBytes, OriginalPolicySetFrameHeader, OriginalPolicySetFrameInput,
    ScannedOriginalPolicySetFrame, prepare_original_policy_set_frame_bytes,
    restore_original_policy_set_frame_header, scan_original_policy_set_frame,
};
pub use patch::{CompleteCorePolicyPatchInput, prepare_complete_core_policy_patch};
pub use presentation::{
    CompletePolicyPresentation, PresentedPolicyState, PresentedRetry, present_complete_policy,
};
pub use public_set::{
    OriginalPolicySetInputRejection, OriginalPolicySetMapOrders, OriginalPolicySetPatchView,
    OriginalPolicySetRequest, encode_original_policy_set_input_rejection_body,
    prepare_original_policy_set_request,
};
pub use replace_content::{
    CompletePolicyReplaceRejection, CompletePolicyReplaceRejectionKind,
    check_complete_policy_replace_content, encode_complete_policy_replace_rejection_body,
};
pub use replace_request::{
    CompletePolicyReplaceApplyError, CompletePolicyReplaceApplyView, CompletePolicyReplaceFresh,
    OriginalCompletePolicyReplaceApplyError, OriginalCompletePolicyReplaceApplyView,
    prepare_complete_policy_replace_fresh, prepare_original_complete_policy_replace_fresh,
};
pub use replace_request::{
    OriginalFlowPolicyInstall, OriginalFlowPolicyReplaceFresh, OriginalFlowPolicyReplaceRejection,
    prepare_original_flow_policy_replace_fresh,
};
pub use replace_request::{
    OriginalPolicySetApplyView, OriginalPolicySetFresh, prepare_original_policy_set_fresh,
};
pub use request::{
    CompleteCorePolicyPatchApplyError, CompleteCorePolicyPatchApplyView,
    CompleteCorePolicyPatchFresh, prepare_complete_core_policy_patch_fresh,
};
pub use resolution::{ResolvedGovernance, resolve_governance};
pub use restoration::{restore_complete_policy, validate_stored_complete_policy};
pub use retention_input::{
    PolicyRetentionInputRejection, encode_complete_policy_retention_input_rejection_body,
    normalize_complete_policy_retention_override_input,
};
pub use retry_input::{
    PolicyRetryInputRejection, RawPolicyRetryValue, normalize_complete_policy_retry_input,
    normalize_complete_policy_retry_override_input,
};
pub use rules::{ResolvedBudget, ResolvedCircuit};

/// Owned complete normalized content; supplied empty groups remain present.
///
/// Construction and refused-owner destruction are outside logical schema work.
/// The outer preparation layer must bound both before supplying these owners.
#[derive(Eq, PartialEq)]
pub struct CompletePolicyData {
    /// Nonempty opaque workflow type, bounded by complete actual bytes.
    pub workflow_type: Vec<u8>,
    /// Optional customer label or the entire Rust nonnegative numeric range.
    pub version: Option<PolicyVersion>,
    /// Captured positive active duration; absence means infinity.
    pub max_active_ms: Option<u64>,
    /// Presence-preserving normalized type retry override.
    pub retry: Option<RetryOverride>,
    /// Presence-preserving normalized type retention override.
    pub retention: Option<RetentionOverride>,
    /// Unique opaque logical states with their own attached policy content.
    pub states: BTreeMap<Vec<u8>, CompleteStatePolicyData>,
    /// Optional canonical attribute names, including a supplied empty list.
    pub indexed_attributes: Option<Vec<Vec<u8>>>,
    /// Optional single canonical indexed state-metadata key.
    pub indexed_state_meta: Option<Vec<u8>>,
    /// Optional complete type governance, including a supplied empty group.
    pub governance: Option<GovernanceData>,
}

/// Admitted owned normalized content with an explicit internal generation.
#[derive(Eq, PartialEq)]
pub struct CompletePolicyCandidate {
    generation: u64,
    data: CompletePolicyData,
}

impl CompletePolicyCandidate {
    /// Reads the supplied validated generation; it is never allocated here.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Borrows the original normalized content without resolving or cloning it.
    #[must_use]
    pub const fn data(&self) -> &CompletePolicyData {
        &self.data
    }
}

/// Candidate bound to actual envelope owners with separately checked lengths.
///
/// An outer private codec must prove content and namespace correspondence.
/// The snapshot range is not retained or reconstructed by this value.
#[derive(Eq, PartialEq)]
pub struct CompletePolicyValue {
    candidate: CompletePolicyCandidate,
    encoded: Vec<u8>,
}

impl CompletePolicyValue {
    /// Reads the explicit candidate generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.candidate.generation
    }

    /// Borrows admitted normalized content with the containing owner's lifetime.
    #[must_use]
    pub const fn data(&self) -> &CompletePolicyData {
        &self.candidate.data
    }

    /// Borrows the actual complete envelope without claiming a content proof.
    #[must_use]
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
}

/// Known effect content borrowed from the original complete policy owner.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ResolvedEffects<'v> {
    allowed: &'v [Vec<u8>],
    denied: &'v [Vec<u8>],
    require_idempotency_key: bool,
}

impl<'v> ResolvedEffects<'v> {
    /// Borrows allowed names in their original normalized order.
    #[must_use]
    pub const fn allowed(self) -> &'v [Vec<u8>] {
        self.allowed
    }

    /// Borrows denied names in their original normalized order.
    #[must_use]
    pub const fn denied(self) -> &'v [Vec<u8>] {
        self.denied
    }

    /// Reads the resolved idempotency-key requirement.
    #[must_use]
    pub const fn require_idempotency_key(self) -> bool {
        self.require_idempotency_key
    }
}

/// Known audit scalar content; no event or denial service is executed.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ResolvedAudit {
    effect_events: bool,
    denials: DenialAuditMode,
}

impl ResolvedAudit {
    /// Reads whether effect events are requested.
    #[must_use]
    pub const fn effect_events(self) -> bool {
        self.effect_events
    }

    /// Reads the requested denial-audit mode.
    #[must_use]
    pub const fn denials(self) -> DenialAuditMode {
        self.denials
    }
}

/// Known approval content borrowed from the original policy owner.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ResolvedApprovals<'v> {
    effects: &'v [Vec<u8>],
    states: &'v [Vec<u8>],
    assignees: &'v [Vec<u8>],
    timeout_ms: Option<u64>,
}

impl<'v> ResolvedApprovals<'v> {
    /// Borrows effects requiring approval.
    #[must_use]
    pub const fn effects(self) -> &'v [Vec<u8>] {
        self.effects
    }

    /// Borrows approval-associated states.
    #[must_use]
    pub const fn states(self) -> &'v [Vec<u8>] {
        self.states
    }

    /// Borrows the original assignee names without executing assignment.
    #[must_use]
    pub const fn assignees(self) -> &'v [Vec<u8>] {
        self.assignees
    }

    /// Reads the optional positive approval timeout.
    #[must_use]
    pub const fn timeout_ms(self) -> Option<u64> {
        self.timeout_ms
    }
}
