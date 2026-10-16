//! Recording submission controls; retained fixtures confer no real durability.

#![forbid(unsafe_code)]

use wf_application::commands::independent::{
    IndependentMutationCall, MutationCallLimits, MutationItemInput, MutationItemStatus,
    MutationReplyMode, submit_independent_mutations,
};
use wf_application::commands::{IntentInput, IntentLimits, IntentValue, prepare_intent};
use wf_domain::workflow::DomainError;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::caller::{
    MutationCallerError, MutationFieldValue, MutationInput, NormalizedMutation,
    normalize_core_mutation,
};
use wf_domain::workflow::indexed::IndexRejection;
use wf_domain::workflow::request::{
    ClientIntent, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
};
use wf_ports::consensus::{
    MutationNotSubmitted, MutationSubmission, MutationSubmitPort, MutationUnknown,
};
use wf_ports::log::{
    CodecLimits, PreparedBytes, prepare_public_mutation, prepare_public_mutation_intent,
};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity, TimestampMillis,
};

mod normalization;
mod submission;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const MIB: usize = 1_024 * 1_024;

struct ProbeMutationPort {
    events: Vec<&'static str>,
    frames: Vec<Vec<u8>>,
    identities: Vec<(RequestScope, RequestId, ClientIntent)>,
    replies: Vec<(RetainedStatus, Vec<u8>)>,
    unknown_at: Option<usize>,
    denied_at: Option<usize>,
    prepare_failure_at: Option<usize>,
    foreign_intent: bool,
    time: u64,
}

fn probe() -> ProbeMutationPort {
    ProbeMutationPort {
        events: Vec::new(),
        frames: Vec::new(),
        identities: Vec::new(),
        replies: Vec::new(),
        unknown_at: None,
        denied_at: None,
        prepare_failure_at: None,
        foreign_intent: false,
        time: 2_000,
    }
}

fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    ))
}

fn clock(now: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?)
}

fn fields() -> [(&'static str, MutationFieldValue<'static>); 2] {
    [
        ("lease_token", MutationFieldValue::Bytes(b"l")),
        ("fencing_token", MutationFieldValue::Integer(1)),
    ]
}

fn item<'a>(
    scope: &'a RequestScope,
    request: &'a RequestId,
    id: &'a [u8],
    fields: &'a [(&'a str, MutationFieldValue<'a>)],
) -> MutationItemInput<'a> {
    MutationItemInput {
        scope,
        request,
        input: MutationInput {
            operation: "complete",
            id,
            fields,
        },
    }
}

fn many<'a>(
    items: &'a [MutationItemInput<'a>],
    reply: MutationReplyMode,
) -> IndependentMutationCall<'a> {
    IndependentMutationCall::TerminalMany {
        operation: "complete",
        base_partition: Some(b"p"),
        common: &[],
        items,
        reply,
    }
}

fn hex(text: &str) -> TestResult<Vec<u8>> {
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

impl MutationSubmitPort for ProbeMutationPort {
    fn prepare(
        &mut self,
        _scope: &RequestScope,
        _request: &RequestId,
        _caller: &NormalizedMutation<'_>,
    ) -> Result<CommittedTime, MutationNotSubmitted> {
        self.events.push("prepare");
        if self.prepare_failure_at == Some(self.frames.len()) {
            return Err(MutationNotSubmitted::Preparation);
        }
        clock(self.time).map_err(|_| MutationNotSubmitted::Preparation)
    }

    fn submit(
        &mut self,
        scope: &RequestScope,
        request: &RequestId,
        prepared: PreparedBytes,
        workspace: usize,
    ) -> MutationSubmission {
        self.events.push("submit");
        let index = self.frames.len();
        if self.denied_at == Some(index) {
            return MutationSubmission::NotSubmitted(MutationNotSubmitted::InvalidScope);
        }
        let codec = CodecLimits::new(AdmissionLimits::default());
        let Ok(decoded) = prepare_public_mutation(prepared.as_bytes(), codec, workspace) else {
            return MutationSubmission::Unknown(MutationUnknown::InvalidReply);
        };
        let intent = decoded.request().intent;
        self.identities
            .push((scope.clone(), request.clone(), intent));
        self.frames.push(prepared.as_bytes().to_vec());
        if self.unknown_at == Some(index) {
            return MutationSubmission::Unknown(MutationUnknown::CommitUncertain);
        }
        let (status, body) = self
            .replies
            .get(index)
            .cloned()
            .unwrap_or((RetainedStatus::Accepted, vec![0]));
        let metadata = RetainedMetadata {
            scope: scope.clone(),
            request: request.clone(),
            intent: if self.foreign_intent {
                ClientIntent::new(1, [0xfe; 32]).unwrap_or(intent)
            } else {
                intent
            },
            original_position: LogIdentity::new(
                1,
                7,
                AppliedPosition::new(u64::try_from(index).unwrap_or(u64::MAX)),
            ),
            original_time: TimestampMillis::new(2_000),
            expires_at: TimestampMillis::new(3_000),
            status,
        };
        match RetainedOutcome::new(metadata, 2, body, AdmissionLimits::default()) {
            Ok(outcome) => MutationSubmission::Retained(Box::new(outcome)),
            Err(_) => MutationSubmission::Unknown(MutationUnknown::InvalidReply),
        }
    }
}
