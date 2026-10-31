//! Actual native log/state integration; no production Raft/supervisor claim.
use super::{TestResult, f, p, q};
use std::sync::Arc;
use wf_application::commands::claim::{
    PublicClaimCallLimits as Limits, PublicClaimCallRequest as Request, PublicClaimCallResult as R,
    retry_public_claim_call, submit_public_claim_call,
};
use wf_domain::{
    claim::caller::{
        ClaimCallerLimits, ClaimDueInput, ClaimPartitionsInput, ClaimStatesInput,
        NormalizedClaimDue, normalize_claim_due,
    },
    workflow::request::{ClientIntent, RequestScope, RetainedOutcome},
};
use wf_ports::{
    consensus::{
        MutationNotSubmitted as N, MutationSubmission as M, MutationUnknown as U,
        PublicClaimReadLimits, PublicClaimRequestLookup as Lookup, PublicClaimSubmitPort,
    },
    log::{PreparedBytes, RaftLogStore},
    state::{GroupApplyStore, GroupApplyTransaction},
};
use wf_storage::{log::RedbLogStore, state::RedbStateStore};
use wf_types::{identity::RequestId, version::CommittedTime};
mod faults;
mod fixtures;
mod flow;
mod ports;
