//! Explicit single-caller preparation context and enclosing resource bounds.

use wf_domain::workflow::request::RequestScope;
use wf_types::identity::RequestId;
use wf_types::version::CommittedTime;

use crate::consensus::{WorkflowValueDefaults, WorkflowValueItemInput, WorkflowValuePolicyWitness};
use crate::log::CodecLimits;

/// Owned observations associated with one logical caller, not authority proof.
pub struct LogicalMutationFrameContext {
    /// Caller-bound original tenant/group/recovery namespace.
    pub scope: RequestScope,
    /// Stable request identity, separate from comparison intent.
    pub request: RequestId,
    /// Explicit server preparation stamp, without reading a clock here.
    pub stamp: CommittedTime,
    /// Original presence and value of the captured-reference marker.
    pub policy_references_captured: Option<bool>,
    /// Captured lifecycle configuration and its exact content identity.
    pub defaults: WorkflowValueDefaults,
    /// Owned ordered witnesses, revalidated against authority at apply.
    pub policies: Vec<WorkflowValuePolicyWitness>,
    /// Independent source input; all caller-presence facts must agree.
    pub input: WorkflowValueItemInput,
}

/// Positive enclosing binding/encoding caps, separate from native RSS or OOM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogicalMutationFrameLimits {
    /// Existing exact frame/canonical/inline schema profile.
    pub codec: CodecLimits,
    /// Incoming capacities plus conservative requested simultaneous ownership.
    pub maximum_owned: usize,
    /// Reported caller preparation, binding and conservative codec work.
    pub maximum_work: usize,
}
