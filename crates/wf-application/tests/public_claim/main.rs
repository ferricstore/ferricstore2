//! Submission-contract controls; recording fixtures confer no runtime authority.
#![forbid(unsafe_code)]
use std::sync::Arc;
use wf_application::commands::claim::*;
use wf_domain::{
    claim::caller::{
        ClaimCallerLimits, ClaimDueInput, ClaimPartitionsInput, ClaimStatesInput,
        NormalizedClaimDue, normalize_claim_due,
    },
    workflow::request::{
        ClientIntent, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
    },
};
use wf_ports::{
    consensus::{
        MutationNotSubmitted as N, MutationSubmission as M, MutationUnknown as U,
        PublicClaimReadLimits, PublicClaimRequestLookup as L, PublicClaimSubmitPort,
    },
    log::{CodecLimits, PreparedBytes, prepare_public_claim_intent, prepare_public_claim_request},
};
use wf_types::{
    identity::{GroupId, RecoveryId, RequestId, TenantId},
    version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity, TimestampMillis},
};
mod admission;
mod binding;
mod fixtures;
mod original;
mod retry;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
