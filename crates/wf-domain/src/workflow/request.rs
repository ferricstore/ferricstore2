//! Scoped request intent/replay over borrowed authoritative retained outcomes.

use std::collections::BTreeMap;

use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::CommittedTime;

use super::admission::{AdmissionError, AdmissionLimits};
use super::batch::{
    ApplyError, BatchDecision, BatchItem, BatchRejection, GroupView, decide_group_batch,
};
use super::control::{ControlPlan, ControlState, EntryIdentity};
use gate::{CommittedHeader, GateDecision, GateView, inspect_request};

mod claim;
mod dispatch;
pub mod gate;
pub mod indexed;
mod outcome;
pub mod policy;
pub use claim::decide_claim_request;
pub use dispatch::{DispatchDecision, DispatchPlan};
pub use outcome::{
    PolicyRejection, RequestValueError, RetainedMetadata, RetainedOutcome, RetainedStatus,
};

/// Stable server-bound tenant/group/recovery namespace, retained across retries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestScope {
    tenant: TenantId,
    group: GroupId,
    recovery: RecoveryId,
}

impl RequestScope {
    /// Combines validated opaque identities; current authorization stays outer.
    #[must_use]
    pub const fn new(tenant: TenantId, group: GroupId, recovery: RecoveryId) -> Self {
        Self {
            tenant,
            group,
            recovery,
        }
    }
    /// Borrows tenant identity, never credentials.
    #[must_use]
    pub const fn tenant(&self) -> &TenantId {
        &self.tenant
    }
    /// Borrows original group identity, independently of client routing hints.
    #[must_use]
    pub const fn group(&self) -> &GroupId {
        &self.group
    }
    /// Borrows recovery identity; disaster restore creates a new namespace.
    #[must_use]
    pub const fn recovery(&self) -> &RecoveryId {
        &self.recovery
    }
}

/// Versioned canonical client-intent digest, excluding prepared leader inputs.
///
/// Application verifies version-1 canonical bytes and SHA-256 before constructing
/// this explicit committed value. Public DTO/intent mapping and private committed
/// encoding are separate contracts. The domain performs no hash or encoding I/O.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClientIntent {
    version: u16,
    digest: [u8; 32],
}

impl ClientIntent {
    /// Admits the selected canonical intent version and explicit verified digest.
    ///
    /// # Errors
    /// Rejects an unsupported canonical default/field-normalization version.
    pub const fn new(version: u16, digest: [u8; 32]) -> Result<Self, RequestValueError> {
        if version == 1 {
            Ok(Self { version, digest })
        } else {
            Err(RequestValueError::UnsupportedVersion)
        }
    }
    /// Selects the mathematical complete-core-Patch grammar for an explicit digest.
    ///
    /// Performs no verification, admission, payment or authorization and proves
    /// no committed outcome. Frame callers must verify the actual original bytes;
    /// persisted-outcome callers must validate their full framing and response.
    #[must_use]
    pub const fn complete_core_policy_patch(digest: [u8; 32]) -> Self {
        Self { version: 2, digest }
    }

    /// Selects the complete-Replace registry after same-frame digest comparison.
    #[must_use]
    pub(crate) const fn complete_policy_replace(digest: [u8; 32]) -> Self {
        Self { version: 2, digest }
    }

    /// Selects the original whole-flow registry after same-frame digest comparison.
    #[must_use]
    pub(crate) const fn original_flow_policy_replace(digest: [u8; 32]) -> Self {
        Self { version: 3, digest }
    }

    /// Selects the mathematical original public-SET grammar for an explicit digest.
    /// Constructs the original SET mathematical digest identity only.
    /// No verification, admission, payment, runtime origin or apply authority is supplied.
    #[must_use]
    pub const fn original_policy_set(digest: [u8; 32]) -> Self {
        Self { version: 4, digest }
    }

    /// Selects the prepared mutation-batch grammar; grants no apply authority.
    #[must_use]
    pub const fn record_batch(digest: [u8; 32]) -> Self {
        Self { version: 5, digest }
    }

    /// Selects ordered independent mutations; grants no apply authority.
    #[must_use]
    pub const fn independent_record_batch(digest: [u8; 32]) -> Self {
        Self { version: 6, digest }
    }

    /// Selects the private atomic-start digest identity only.
    /// No digest verification, framing, admission, payment, runtime origin or
    /// apply authority is supplied by this primitive registry selection.
    #[must_use]
    pub const fn atomic_start(digest: [u8; 32]) -> Self {
        Self { version: 7, digest }
    }

    /// Selects value-batch caller identity without verification or apply authority.
    #[must_use]
    pub const fn workflow_value_batch(digest: [u8; 32]) -> Self {
        Self { version: 8, digest }
    }

    /// Selects explicit stored-byte caller identity without apply authority.
    #[must_use]
    pub const fn workflow_stored_value_batch(digest: [u8; 32]) -> Self {
        Self { version: 9, digest }
    }

    /// Selects the private singleton Step identity without verification/authority.
    /// The explicit codec must derive and verify the actual represented fields;
    /// this mathematical registry selection supplies no admission or submission.
    #[must_use]
    pub const fn step_continue(digest: [u8; 32]) -> Self {
        Self {
            version: 10,
            digest,
        }
    }

    /// Selects stored-payload Step identity without verification or authority.
    #[must_use]
    pub const fn stored_step_continue(digest: [u8; 32]) -> Self {
        Self {
            version: 11,
            digest,
        }
    }

    /// Selects private identified-credit identity without verification/authority.
    /// The explicit codec must bind the complete actual frame before apply.
    #[must_use]
    pub const fn governance_credit(digest: [u8; 32]) -> Self {
        Self {
            version: 12,
            digest,
        }
    }

    /// Reads canonical intent normalization version.
    #[must_use]
    pub const fn version(self) -> u16 {
        self.version
    }
    /// Borrows stable client intent, independently of prepared identity.
    #[must_use]
    pub const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}

/// One coherent tenant/group/recovery apply view, with retained outcome authority.
#[derive(Clone, Copy, Debug)]
pub struct RequestView<'a> {
    /// Server-bound namespace for both current records and retained outcomes.
    pub scope: &'a RequestScope,
    /// Current persisted control identity/time/checkpoint.
    pub control: ControlState,
    /// Complete authoritative group-local record/placement values.
    pub group: GroupView<'a>,
    /// Unique retained outcomes in exactly this request namespace.
    pub outcomes: &'a BTreeMap<RequestId, RetainedOutcome>,
}

/// Explicit immutable inputs verified by private committed-byte preparation.
#[derive(Clone, Copy, Debug)]
pub struct CommittedEnvelope<'a> {
    /// Private committed command schema version, separate from public wire.
    pub format_version: u16,
    /// Original server-bound scope, retained through redirects/retries.
    pub scope: &'a RequestScope,
    /// Stable caller request identity within this original scope.
    pub request: &'a RequestId,
    /// Canonical caller intent; excludes leader time/generated inputs.
    pub intent: ClientIntent,
    /// Full log identity and separately verified prepared-entry digest.
    pub identity: EntryIdentity,
    /// Validated leader-supplied trusted stamp; public clients cannot forge it.
    pub stamp: CommittedTime,
    /// Actual immutable prepared bytes; decoder verifies every supplied field.
    pub encoded_entry: &'a [u8],
    /// Actual isolation-scope encoding; outer decoder verifies typed scope binding.
    pub encoded_isolation_scope: &'a [u8],
    /// Bounded source atomic batch, with no implicit whole-partition copy.
    pub items: &'a [BatchItem<'a>],
}
impl<'a> CommittedEnvelope<'a> {
    /// Borrows the reusable header without exposing current operation guards.
    #[must_use]
    pub const fn header(self) -> CommittedHeader<'a> {
        CommittedHeader {
            format_version: self.format_version,
            scope: self.scope,
            request: self.request,
            intent: self.intent,
            identity: self.identity,
            stamp: self.stamp,
            encoded_entry: self.encoded_entry,
            encoded_isolation_scope: self.encoded_isolation_scope,
        }
    }
}

/// Request decision; replay borrows the original response owner unchanged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestDecision<'a> {
    /// First request needs one retained accepted/rejected outcome after persistence.
    Fresh(BatchDecision),
    /// Equal canonical intent returns exact original bytes/status/time/position.
    Replay(&'a RetainedOutcome),
    /// Stable request ID was reused for different intent; preserve original outcome.
    IntentConflict,
    /// Original retention guarantee expired; never reexecute this retained identity.
    Expired,
    /// Same full last log identity/digest/stamp; no state/control/outcome writes.
    LogReplay,
}

/// Proposed control and business values; atomic persistence is an adapter duty.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestPlan<'a> {
    control: ControlPlan,
    decision: RequestDecision<'a>,
}

impl<'a> RequestPlan<'a> {
    /// Reads current processing control separately from an original retained result.
    #[must_use]
    pub const fn control(&self) -> ControlPlan {
        self.control
    }
    /// Borrows the fresh/replayed/rejected request decision.
    #[must_use]
    pub const fn decision(&self) -> &RequestDecision<'a> {
        &self.decision
    }
    /// Only fresh accepted/deterministic-rejected requests create retained outcomes.
    #[must_use]
    pub const fn requires_outcome_write(&self) -> bool {
        matches!(self.decision, RequestDecision::Fresh(_))
    }
    /// Transfers bounded decision and control values without cloning replay bytes.
    #[must_use]
    pub fn into_parts(self) -> (ControlPlan, RequestDecision<'a>) {
        (self.control, self.decision)
    }
}

/// Decides scoped request replay before any changed workflow guard/index lookup.
///
/// Current authorization is required before this API. The outer decoder verifies
/// canonical intent/prepared digest, actual bytes and complete scope. It atomically
/// persists fresh result bytes and control with workflow/index/history plans.
/// Host errors preserve all authority and cannot become retained business outcomes.
///
/// # Errors
/// Rejects foreign/corrupt authority, unsupported committed format, conflicting log
/// identity and invalid fresh execution time as apply failure.
pub fn decide_request<'a>(
    view: RequestView<'a>,
    envelope: CommittedEnvelope<'a>,
    limits: AdmissionLimits,
) -> Result<RequestPlan<'a>, ApplyError> {
    if view.group.group != view.scope.group() {
        return Err(ApplyError::InvalidAuthority);
    }
    let gate = inspect_request(
        GateView {
            scope: view.scope,
            control: view.control,
            outcomes: view.outcomes,
        },
        envelope.header(),
        limits,
    )?;
    let control = gate.control();
    let decision = match gate.decision() {
        GateDecision::Replay(retained) => RequestDecision::Replay(retained),
        GateDecision::IntentConflict => RequestDecision::IntentConflict,
        GateDecision::Expired => RequestDecision::Expired,
        GateDecision::LogReplay => RequestDecision::LogReplay,
        GateDecision::Fresh => {
            if envelope.items.is_empty() || envelope.items.len() > limits.batch_items() {
                RequestDecision::Fresh(BatchDecision::Rejected(BatchRejection::Admission(
                    AdmissionError::BatchItems,
                )))
            } else {
                if envelope
                    .items
                    .iter()
                    .any(|item| item.command.now != control.execution_time())
                {
                    return Err(ApplyError::InvalidAuthority);
                }
                RequestDecision::Fresh(decide_group_batch(
                    view.group,
                    envelope.items,
                    envelope.encoded_entry,
                    limits,
                )?)
            }
        }
    };
    Ok(RequestPlan { control, decision })
}
