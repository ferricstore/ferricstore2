//! Prepared command codecs and consensus-neutral durable log capabilities.

use std::fmt;

use sha2::{Digest, Sha256};
use wf_domain::policy::installation::{PolicyCandidate, PolicyLimits, PolicyValue};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::PreparedDigest;
use wf_domain::workflow::request::ClientIntent;
use wf_domain::workflow::request::gate::CommittedHeader;
use wf_types::version::LogIdentity;

use crate::consensus::{PreparedOperation, PreparedRequest};

mod claim;
pub use claim::{
    ClaimApplyDecision, ClaimApplyPlan, ClaimPhaseInspection, FreshClaimPhase,
    claim_response_matches, claim_retained_summary, decide_fresh_claim_phase_observed,
    inspect_claim_phase_point, prepare_claim_phase,
};
mod common;
pub use common::normal_recovery_header;
mod complete_policy;
mod control;
mod decode;
mod encode;
mod keys;
mod policy;
mod public_mutation;
mod reader;
mod record_batch;
mod scan;
mod storage;
pub use control::{
    decode_raft_membership_body, encode_raft_blank, encode_raft_membership, measure_control_entry,
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
    RecordBatchInspection, decide_fresh_independent_record_batch_touched,
    decide_fresh_record_batch, decide_fresh_record_batch_touched, decide_record_batch,
    encode_independent_record_batch, encode_record_batch,
    independent_record_batch_response_matches, inspect_independent_record_batch_point,
    inspect_record_batch, inspect_record_batch_point, prepare_independent_record_batch,
    prepare_record_batch, record_batch_retained_status, record_batch_retained_summary,
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

/// Immutable byte/count budgets shared by encoder, decoder and ordered apply.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CodecLimits {
    admission: AdmissionLimits,
    policy: Option<PolicyLimits>,
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
    bytes: Vec<u8>,
    digest: PreparedDigest,
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

/// Decoded typed request bound to actual immutable input bytes.
pub struct DecodedRequest<'a> {
    request: PreparedRequest,
    encoded: &'a [u8],
    digest: PreparedDigest,
    isolation: Vec<u8>,
    state_keys: Vec<Vec<u8>>,
    claim_filters: Option<claim::ClaimFilterRefs<'a>>,
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
            Operation::StepContinue(_) => return None,
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

/// Operational preparation/corruption failure; never a retained business outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    /// Private command version is unsupported.
    UnsupportedVersion,
    /// An operation/backoff discriminator is unknown.
    UnsupportedTag,
    /// Structurally valid original ETF needs another captured atom profile.
    UnsupportedPolicyProfile,
    /// A present/absent tag is neither zero nor one.
    InvalidOptionality,
    /// Fixed fields, typed identities or input continuity are invalid.
    Malformed,
    /// Bytes remain after the complete represented command.
    TrailingBytes,
    /// Complete input/output or derived-key aggregate exceeds its profile.
    ByteLimit,
    /// A batch is empty or a batch/filter count exceeds its structural profile.
    ItemLimit,
    /// A length-prefixed variable/nested field exceeds its configured byte cap.
    ValueLimit,
    /// Complete derived state key exceeds 65,535 bytes.
    KeyLimit,
    /// Complete derived isolation scope exceeds 8,192 bytes.
    ScopeLimit,
    /// A checked length/count cannot fit its explicit representation.
    LengthOverflow,
    /// A safe fallible builder reservation failed.
    Allocation,
}

impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedVersion => "unsupported private prepared version",
            Self::UnsupportedTag => "unsupported private prepared tag",
            Self::UnsupportedPolicyProfile => "unsupported original policy profile",
            Self::InvalidOptionality => "invalid private prepared optionality",
            Self::Malformed => "malformed private prepared command",
            Self::TrailingBytes => "trailing private prepared bytes",
            Self::ByteLimit => "private prepared byte budget exceeded",
            Self::ItemLimit => "private prepared item budget exceeded",
            Self::ValueLimit => "private prepared field budget exceeded",
            Self::KeyLimit => "private prepared state key too large",
            Self::ScopeLimit => "private prepared isolation scope too large",
            Self::LengthOverflow => "private prepared length overflow",
            Self::Allocation => "private prepared allocation failed",
        })
    }
}
impl std::error::Error for CodecError {}

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
    scan::preflight(encoded, limits)?;
    decode::read(encoded, limits)
}

fn digest(bytes: &[u8]) -> PreparedDigest {
    PreparedDigest::new(Sha256::digest(bytes).into())
}
