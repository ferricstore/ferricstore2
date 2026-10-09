//! Typed single-record commands with explicit committed inputs.

use wf_types::identity::RecordId;
use wf_types::version::{Fence, TimestampMillis};

use crate::claim::Claim;
use crate::lease::{ExtendLease, LeaseGuard, LeaseToken};
use crate::retry::{PolicyRetry, Retry};

/// Minimal creation inputs; ordinary create rejects an existing identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Create {
    /// Nonempty binary workflow type.
    pub workflow_type: Vec<u8>,
    /// Nonempty binary initial state, defaulted by the caller when appropriate.
    pub state: Vec<u8>,
    /// Explicit deadline, or committed time when omitted.
    pub run_at: Option<TimestampMillis>,
    /// Inline optional payload; empty bytes differ from omission.
    pub payload: Option<Vec<u8>>,
}

/// Nonterminal state change guarded by current state, fence and optional lease.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transition {
    /// Required expected lifecycle state.
    pub from: Vec<u8>,
    /// Nonterminal destination state; `running` is forbidden.
    pub to: Vec<u8>,
    /// Current fencing generation.
    pub fence: Fence,
    /// Current token for a leased record; omission for an unleased record.
    pub lease_token: Option<LeaseToken>,
    /// Explicit deadline, defaulting to committed time.
    pub run_at: Option<TimestampMillis>,
    /// Optional new payload; omission retains the existing payload.
    pub payload: Option<Vec<u8>>,
}

/// Inputs shared by complete and fail, whose effect remains distinct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Terminal {
    /// Current ownership proof, or fence for a permitted terminal duplicate.
    pub guard: LeaseGuard,
    /// Optional replacement payload.
    pub payload: Option<Vec<u8>>,
    /// Result for complete, error for fail; omission clears that observation.
    pub value: Option<Vec<u8>>,
}

/// Explicit cancellation, with optional optimistic state and version guards.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cancel {
    /// Current fencing generation.
    pub fence: Fence,
    /// Current lease token, required only when the record owns one.
    pub lease_token: Option<LeaseToken>,
    /// Optional expected lifecycle state.
    pub expected_state: Option<Vec<u8>>,
    /// Optional expected record version.
    pub expected_version: Option<u64>,
    /// Optional inline cancellation error/reason.
    pub error: Option<Vec<u8>>,
}

/// Only the implemented single-record operation surface is representable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    /// Create one new record.
    Create(Create),
    /// Claim a named due unleased record, without selecting a partition candidate.
    Claim(Claim),
    /// Claim a named expired running record, preserving its logical run state.
    Reclaim(Claim),
    /// Change to a nonterminal state.
    Transition(Transition),
    /// Complete the current run.
    Complete(Terminal),
    /// Fail the current run.
    Fail(Terminal),
    /// Schedule a retry with caller-resolved time.
    Retry(Retry),
    /// Retry with defaults and a coherent authoritative installed policy view.
    RetryWithPolicy(PolicyRetry),
    /// Replace the current lease deadline.
    ExtendLease(ExtendLease),
    /// Cancel with state/version/lease/fence checks.
    Cancel(Cancel),
}

/// A command already supplied with deterministic committed time and identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommittedCommand {
    /// Identity of the only affected record.
    pub id: RecordId,
    /// Explicit committed milliseconds; no clock is consulted.
    pub now: TimestampMillis,
    /// Typed operation and caller-prepared values.
    pub operation: Operation,
}
