//! Prepared command codecs and consensus-neutral durable log capabilities.

use sha2::{Digest, Sha256};
use wf_domain::policy::installation::{PolicyCandidate, PolicyValue};
use wf_domain::workflow::control::PreparedDigest;
use wf_domain::workflow::request::ClientIntent;
use wf_domain::workflow::request::gate::CommittedHeader;
use wf_types::version::LogIdentity;

use crate::consensus::{PreparedOperation, PreparedRequest};

mod catalog_ack;
mod catalog_membership;
mod catalog_progress;
mod catalog_prune;
pub use catalog_membership::{
    PreparedCatalogMembershipRequest, catalog_membership_recovery_header,
    catalog_membership_request_intent, catalog_membership_retained_summary,
    encode_catalog_membership_request, prepare_catalog_membership_request,
};
pub use catalog_progress::{
    PreparedCatalogProgressRequest, catalog_progress_recovery_header,
    catalog_progress_request_intent, catalog_progress_retained_summary,
    encode_catalog_progress_request, prepare_catalog_progress_request,
};
pub use catalog_prune::{
    PreparedCatalogPruneRequest, catalog_prune_recovery_header, catalog_prune_request_intent,
    catalog_prune_retained_summary, encode_catalog_prune_reply, encode_catalog_prune_request,
    prepare_catalog_prune_request,
};
mod claim;
pub use catalog_ack::{
    PreparedCatalogAckRequest, catalog_ack_recovery_header, catalog_ack_request_intent,
    catalog_ack_retained_summary, decode_catalog_ack_reply, encode_catalog_ack_reply,
    encode_catalog_ack_request, prepare_catalog_ack_request, verify_catalog_ack_reply,
};
pub use claim::{
    ClaimApplyDecision, ClaimApplyPlan, ClaimPhaseInspection, FreshClaimPhase,
    claim_response_matches, claim_retained_summary, decide_fresh_claim_phase_observed,
    inspect_claim_phase_point, prepare_claim_phase,
};
mod common;
mod release;
pub use common::{CodecError, normal_recovery_header, stored_value_recovery_header};
pub use common::{step_continue_recovery_header, stored_step_continue_recovery_header};
pub use record_batch::{encode_stored_step_continue, prepare_stored_step_continue};
pub use release::{
    ReleaseBindingRef, ReleaseReconcileCursor, decode_release_cursor, encode_release_binding,
    encode_release_cursor, encode_release_intent, encode_release_meta, inspect_release_binding,
    inspect_release_intent, inspect_release_meta,
};
mod complete_policy;
mod control;
mod credit;
pub use credit::{
    PreparedCreditRequest, credit_cleanup_intent, credit_cleanup_retained_summary,
    credit_recovery_header, credit_retained_summary, decode_credit_owner, decode_credit_reply,
    encode_credit_owner, encode_credit_reply, encode_credit_reply_parts, encode_credit_request,
    prepare_credit_request, verify_credit_reply_parts, with_public_credit_reply,
};
mod decode;
mod encode;
mod keys;
mod policy;
mod public_mutation;
mod reader;
mod record_batch;
mod scan;
mod storage;
mod value_batch;
pub use control::{
    decode_raft_membership_body, encode_raft_blank, encode_raft_membership, measure_control_entry,
};
pub(crate) use value_batch::response::validate_changes as validate_workflow_value_changes;
pub use value_batch::{
    LogicalMutationFrameContext, LogicalMutationFrameLimits, WorkflowValueDecision,
    WorkflowValueMetadata, decide_fresh_workflow_value_batch_touched,
    decode_workflow_stored_value_batch, decode_workflow_value_batch, encode_logical_mutation_frame,
    encode_workflow_stored_value_batch, encode_workflow_value_batch,
    prepare_workflow_stored_value_batch, prepare_workflow_value_batch,
    workflow_value_retained_summary,
};
mod writer;

pub use storage::{
    RaftLogEntry, RaftLogError, RaftLogLimits, RaftLogState, RaftLogStore, RaftVote,
};

pub use public_mutation::{
    decide_fresh_public_mutation_touched, encode_public_mutation, inspect_public_mutation_point,
    measure_public_mutation, prepare_public_mutation, prepare_public_mutation_intent,
    public_mutation_response_matches, public_mutation_retained_status,
};

pub use record_batch::{
    FreshRecordBatch, RecordBatchApplyDecision, RecordBatchApplyError, RecordBatchApplyPlan,
    RecordBatchInspection, atomic_start_response_matches, atomic_start_retained_summary,
    decide_fresh_atomic_start_touched, decide_fresh_independent_record_batch_touched,
    decide_fresh_record_batch, decide_fresh_record_batch_touched,
    decide_fresh_step_continue_touched, decide_record_batch, encode_atomic_start,
    encode_independent_record_batch, encode_record_batch, encode_step_continue,
    encode_step_continue_response, independent_record_batch_response_matches,
    inspect_atomic_start_point, inspect_independent_record_batch_point, inspect_record_batch,
    inspect_record_batch_point, inspect_step_continue_point, prepare_atomic_start,
    prepare_independent_record_batch, prepare_record_batch, prepare_step_continue,
    record_batch_retained_status, record_batch_retained_summary, step_continue_response_matches,
    step_continue_retained_summary,
};

pub use complete_policy::{
    CompleteCorePolicyPatchIntent, CompleteCorePolicyPatchPointInspection,
    CompleteCorePolicyPatchRequestPlan, CompletePolicyReplaceIntent,
    CompletePolicyReplaceRequestPlan, DecodedCompleteCorePolicyPatchFrame,
    DecodedCompletePolicyReplaceFrame, FreshCompleteCorePolicyPatchFrame,
    FreshOriginalPolicySetFrame, OriginalFlowPolicyReplaceRequestPlan, OriginalPolicyReference,
    OriginalPolicySetPointInspection, OriginalPolicySetRequestPlan,
    VerifiedCompleteCorePolicyPatchFrame, VerifiedCompletePolicyReplaceFrame,
    VerifiedOriginalFlowPolicyReplaceFrame, VerifiedOriginalPolicySetFrame,
    complete_core_policy_patch_response_matches, complete_core_policy_patch_retained_status,
    decide_complete_core_policy_patch_frame, decide_complete_core_policy_patch_point_fresh,
    decide_complete_policy_replace_frame, decide_original_complete_policy_replace_frame,
    decide_original_flow_policy_replace_frame, decide_original_policy_set_frame,
    decide_original_policy_set_frame_bounded, decide_original_policy_set_point_fresh_bounded,
    decode_complete_core_policy_patch_frame, decode_complete_policy_replace_frame,
    inspect_complete_core_policy_patch_point, inspect_original_policy_set_point_frame_bounded,
    original_policy_set_response_matches, original_policy_set_response_status,
    prepare_complete_core_policy_patch_frame, prepare_complete_core_policy_patch_intent,
    prepare_complete_core_policy_patch_point_frame, prepare_complete_policy_replace_frame,
    prepare_complete_policy_replace_intent, prepare_original_complete_policy_reference,
    prepare_original_flow_policy_replace_frame, prepare_original_policy_set_frame,
    prepare_original_policy_set_point_frame_bounded, verify_complete_core_policy_patch_frame,
    verify_complete_policy_replace_frame, verify_original_flow_policy_replace_frame,
    verify_original_policy_set_frame,
};

mod model;
pub use model::{CodecLimits, PreparedBytes};

/// Decoded typed request bound to actual immutable input bytes.
pub struct DecodedRequest<'a> {
    request: PreparedRequest,
    encoded: &'a [u8],
    digest: PreparedDigest,
    isolation: Vec<u8>,
    state_keys: Vec<Vec<u8>>,
    claim_filters: Option<claim::ClaimFilterRefs<'a>>,
    value_metadata: Option<value_batch::ValueBatchMetadata>,
    limits: CodecLimits,
    record_batch_intent: Result<ClientIntent, CodecError>,
}

impl DecodedRequest<'_> {
    /// Borrows the decoded explicit values; current state still requires authority.
    #[must_use]
    pub const fn request(&self) -> &PreparedRequest {
        &self.request
    }
    /// Reads a recomputed identity over exact input bytes.
    #[must_use]
    pub const fn digest(&self) -> PreparedDigest {
        self.digest
    }
    /// Borrows exact input bytes; no complete canonical reencode is required.
    #[must_use]
    pub const fn encoded(&self) -> &[u8] {
        self.encoded
    }
    /// Borrows actual isolation bytes derived from the decoded namespace.
    #[must_use]
    pub fn isolation_scope(&self) -> &[u8] {
        &self.isolation
    }
    /// Borrows the complete derived state key for one represented batch item.
    #[must_use]
    pub fn state_key(&self, index: usize) -> Option<&[u8]> {
        self.state_keys.get(index).map(Vec::as_slice)
    }
    /// Encodes a reached Claim identity using this decoded isolation owner.
    /// The caller admits the exact key owner before invoking this allocation.
    /// # Errors
    /// Rejects another operation, composed-key bounds or allocation failure.
    pub fn claim_state_key(
        &self,
        partition: &wf_types::identity::PartitionId,
        id: &wf_types::identity::RecordId,
    ) -> Result<Vec<u8>, CodecError> {
        if !matches!(self.request.operation, PreparedOperation::ClaimPhase(_)) {
            return Err(CodecError::UnsupportedTag);
        }
        keys::state(&self.isolation, partition.as_bytes(), id.as_bytes())
    }
    /// Borrows the represented payload/result/error fields without another copy.
    #[must_use]
    pub fn value_bytes(&self, index: usize) -> Option<[Option<&[u8]>; 2]> {
        use wf_domain::workflow::Operation;

        let items = self.request.operation.mutation_items()?;
        let operation = &items.get(index)?.command.operation;
        Some(match operation {
            Operation::Create(value) => [value.payload.as_deref(), None],
            Operation::Transition(value) => [value.payload.as_deref(), None],
            Operation::StartAndClaim(value)
                if self.request.intent.version() == 7
                    || (matches!(self.request.intent.version(), 8 | 9)
                        && self
                            .workflow_value_metadata()
                            .is_some_and(|metadata| metadata.profile == 7)) =>
            {
                [value.payload.as_deref(), None]
            }
            Operation::StepContinue(value) if matches!(self.request.intent.version(), 10 | 11) => {
                [value.payload.as_deref(), None]
            }
            Operation::StartAndClaim(_) | Operation::StepContinue(_) => return None,
            Operation::Complete(value) | Operation::Fail(value) => {
                [value.payload.as_deref(), value.value.as_deref()]
            }
            Operation::Retry(value) => [value.payload.as_deref(), value.error.as_deref()],
            Operation::RetryWithPolicy(value) => [value.payload.as_deref(), value.error.as_deref()],
            Operation::Cancel(value) => [value.error.as_deref(), None],
            Operation::Claim(_) | Operation::Reclaim(_) | Operation::ExtendLease(_) => [None, None],
        })
    }
    /// Joins separately assigned log identity to verified prepared-byte identity.
    /// Current authorization and coherent apply authority still precede dispatch.
    #[must_use]
    pub fn header(&self, log: LogIdentity) -> CommittedHeader<'_> {
        CommittedHeader {
            format_version: 1,
            scope: &self.request.scope,
            request: &self.request.request,
            intent: self.request.intent,
            identity: wf_domain::workflow::control::EntryIdentity::new(log, self.digest),
            stamp: self.request.stamp,
            encoded_entry: self.encoded,
            encoded_isolation_scope: &self.isolation,
        }
    }
    /// Borrows the exact decoded policy command/value and original request header.
    #[must_use]
    pub fn policy_envelope(
        &self,
        log: LogIdentity,
    ) -> Option<wf_domain::workflow::request::policy::PolicyEnvelope<'_>> {
        let PreparedOperation::InstallPolicy { command, value } = &self.request.operation else {
            return None;
        };
        Some(wf_domain::workflow::request::policy::PolicyEnvelope {
            header: self.header(log),
            command,
            prepared: value,
        })
    }
    /// Borrows physical query filters and binds separately assigned committed identity.
    /// Selection and current authority are supplied by the ordered apply owner.
    #[must_use]
    pub fn claim_envelope(
        &self,
        log: LogIdentity,
    ) -> Option<wf_domain::claim::apply::ClaimEnvelope<'_>> {
        use wf_domain::claim::PartitionFilter;
        use wf_domain::claim::apply::{ClaimEnvelope, ClaimQuery, ClaimStates};

        let PreparedOperation::ClaimPhase(phase) = &self.request.operation else {
            return None;
        };
        let filters = self.claim_filters.as_ref()?;
        Some(ClaimEnvelope {
            header: self.header(log),
            query: ClaimQuery {
                workflow_type: &phase.query.workflow_type,
                partitions: if phase.query.partitions.is_some() {
                    PartitionFilter::Only(&filters[2])
                } else {
                    PartitionFilter::Any
                },
                states: if phase.query.states.is_some() {
                    ClaimStates::Only(&filters[0])
                } else {
                    ClaimStates::Any
                },
                exclude_states: &filters[1],
                priority: phase.query.priority,
                worker: &phase.query.worker,
                lease_ms: phase.query.lease_ms,
                limit: phase.query.limit,
            },
            token_version: phase.token_version,
            first_ordinal: phase.first_ordinal,
            scan_budget: phase.scan_budget,
            head_budget: phase.head_budget,
            resume: None,
        })
    }
}

/// Prepares one normalized policy envelope without installation or submission.
///
/// # Errors
/// Rejects malformed values or complete private snapshot/envelope limits.
pub fn prepare_policy(
    candidate: PolicyCandidate,
    limits: CodecLimits,
) -> Result<PolicyValue, CodecError> {
    policy::prepare(candidate, limits)
}

/// Encodes only supported typed preparation values without submission/effects.
///
/// # Errors
/// Rejects unsupported/malformed structure and immutable resource bounds.
pub fn encode_prepared(
    request: &PreparedRequest,
    limits: CodecLimits,
) -> Result<PreparedBytes, CodecError> {
    if let PreparedOperation::InstallPolicy { value, .. } = &request.operation {
        policy::verify(value, limits)?;
    }
    let mut measure = writer::Writer::measure(limits.admission.decision_bytes());
    encode::write(request, &mut measure, limits)?;
    let mut output = writer::Writer::allocate(measure.len(), limits.admission.decision_bytes())?;
    encode::write(request, &mut output, limits)?;
    let bytes = output.finish()?;
    Ok(PreparedBytes {
        digest: digest(&bytes),
        bytes,
    })
}

/// Decodes exactly one bounded private command; public requests cannot use this.
///
/// # Errors
/// Rejects corruption/unsupported structure and immutable resource bounds.
pub fn decode_prepared(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<DecodedRequest<'_>, CodecError> {
    value_batch::read_decoded(encoded, limits)
}

fn digest(bytes: &[u8]) -> PreparedDigest {
    PreparedDigest::new(Sha256::digest(bytes).into())
}

mod public_credit;
pub use public_credit::{
    CreditSpendPreparation, PublicCreditReplyExpectation, encode_public_credit_request,
    fingerprint_credit_policy, generate_public_credit_ids, inspect_public_credit_reply,
    prepare_public_credit_request, public_credit_intent, public_credit_recovery_header,
};
