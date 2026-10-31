//! Public claim planning, without workflow writes or external credit authority.
//!
//! Schedules preserve original callers; coalesced representatives grant no
//! combined spend permission. Projection/confirmed values certify neither
//! governance bytes nor remote ownership. The atomic parent remains separate.

use super::caller::NormalizedClaimDue;
use crate::policy::complete::RunningClaimPolicyVersion;
use crate::workflow::active::LimitEnforcement;
use wf_types::version::TimestampMillis;

mod admission;
mod credit;
mod running;
mod schedule;

pub use admission::{ClaimCoordinationError, ClaimCoordinationLimits};
pub use credit::{
    ClaimCreditBatch, ClaimCreditSettlement, ClaimCreditUse, ClaimExecutionOutcome,
    ClaimSpendOutcome, CreditSettlementDisposition, admit_claim_credit_batch, classify_claim_spend,
    plan_claim_credit_settlement,
};
pub use running::{
    CurrentRunningCreditError, RunningPolicyProjection, admit_running_projection,
    resolve_current_running_credit, resolve_running_credit, running_config_version,
};
pub use schedule::{
    ClaimSchedule, advance_claim_schedule, plan_claim_schedule, plan_coalesced_claim_schedule,
};

/// Physical selection phase, interpreted with coherent apply-time indexes later.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ClaimPhase {
    /// The original normalized logical-state filter.
    Requested,
    /// The original filter with physical running excluded.
    Normal,
    /// Expired physical running only.
    Running,
}
/// Externally observed phase completion; no claim or commit proof is created.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ClaimPhaseCompletion {
    /// Successful accepted count for the exact exposed phase.
    Accepted {
        /// Exposed phase index.
        phase: u8,
        /// Actual accepted count.
        claimed: u64,
    },
    /// Known failure, preserving earlier observations.
    KnownFailure {
        /// Exposed phase index.
        phase: u8,
    },
    /// Unknown commit result; no further phase is scheduled.
    UnknownOutcome {
        /// Exposed phase index.
        phase: u8,
    },
}
/// Arithmetic schedule status, independent of request/outcome persistence.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ClaimScheduleStatus {
    /// An actual phase remains.
    Ready,
    /// Quotas have completed or no remaining quota exists.
    Complete,
    /// A known failure stopped the schedule.
    KnownFailure,
    /// An unknown outcome stopped the schedule.
    UnknownOutcome,
}
/// Borrowed phase description; caller is only a filter representative.
pub struct ScheduledClaimPhase<'c, 'a> {
    index: u8,
    kind: ClaimPhase,
    quota: u64,
    caller: &'c NormalizedClaimDue<'a>,
    uses_initial_reclaim: bool,
}
impl<'c, 'a> ScheduledClaimPhase<'c, 'a> {
    /// Exact ordinal within this arithmetic schedule.
    #[must_use]
    pub const fn index(&self) -> u8 {
        self.index
    }
    /// Selected physical phase.
    #[must_use]
    pub const fn kind(&self) -> ClaimPhase {
        self.kind
    }
    /// Maximum accepted count for this phase.
    #[must_use]
    pub const fn quota(&self) -> u64 {
        self.quota
    }
    /// Original first caller, without combined credit/request authority.
    #[must_use]
    pub const fn caller(&self) -> &'c NormalizedClaimDue<'a> {
        self.caller
    }
}
/// A validated running limit with inherited optional fields.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct RunningLimitRule {
    limit: u64,
    enforcement: Option<LimitEnforcement>,
    lease_size: Option<u64>,
}
impl RunningLimitRule {
    /// Nonnegative credit quantity limit.
    #[must_use]
    pub const fn limit(self) -> u64 {
        self.limit
    }
    /// Explicit enforcement, or type/default inheritance.
    #[must_use]
    pub const fn enforcement(self) -> Option<LimitEnforcement> {
        self.enforcement
    }
    /// Optional positive credit quantity, independent of TTL.
    #[must_use]
    pub const fn lease_size(self) -> Option<u64> {
        self.lease_size
    }
}
/// Resolved scalar rule, created only by the admitted internal aggregator.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ResolvedRunningLimit {
    limit: u64,
    enforcement: LimitEnforcement,
    lease_size: u64,
}
impl ResolvedRunningLimit {
    /// Minimum applicable credit limit.
    #[must_use]
    pub const fn limit(self) -> u64 {
        self.limit
    }
    /// Strict if any applicable rule is strict.
    #[must_use]
    pub const fn enforcement(self) -> LimitEnforcement {
        self.enforcement
    }
    /// Minimum applicable credit quantity.
    #[must_use]
    pub const fn lease_size(self) -> u64 {
        self.lease_size
    }
}
/// Borrowed target meaning; routing/fingerprint verification is outer work.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum RunningCreditDemandKind<'a> {
    /// Requires the versioned type hash and worker routing protocol.
    PolicyDerived {
        /// Original caller workflow type.
        workflow_type: &'a [u8],
        /// Original caller worker.
        worker: &'a [u8],
    },
    /// Explicit caller pair, without owner-spend proof.
    Explicit {
        /// Original admitted scope.
        scope: &'a [u8],
        /// Admitted target shard.
        shard_id: u64,
    },
}
/// Borrowed ordinary-call demand; it cannot authorize coalesced callers.
pub struct RunningCreditDemand<'v, 'a> {
    kind: RunningCreditDemandKind<'a>,
    amount: u64,
    limit: u64,
    lease_size: u64,
    enforcement: LimitEnforcement,
    auto_lease: bool,
    ttl_ms: u64,
    trusted_now: TimestampMillis,
    config_version: Option<u64>,
    policy_version: Option<RunningClaimPolicyVersion<'v>>,
}
impl<'v, 'a> RunningCreditDemand<'v, 'a> {
    /// Original scope meaning, without hash or routing authority.
    #[must_use]
    pub const fn kind(&self) -> RunningCreditDemandKind<'a> {
        self.kind
    }
    /// Original ordinary caller limit.
    #[must_use]
    pub const fn amount(&self) -> u64 {
        self.amount
    }
    /// Resolved available-credit policy limit.
    #[must_use]
    pub const fn limit(&self) -> u64 {
        self.limit
    }
    /// Credit prefetch quantity, never a duration.
    #[must_use]
    pub const fn lease_size(&self) -> u64 {
        self.lease_size
    }
    /// Resolved external-owner enforcement.
    #[must_use]
    pub const fn enforcement(&self) -> LimitEnforcement {
        self.enforcement
    }
    /// Whether an external owner may consider automatic leasing.
    #[must_use]
    pub const fn auto_lease(&self) -> bool {
        self.auto_lease
    }
    /// Exact caller lease duration, independent of credit quantity.
    #[must_use]
    pub const fn ttl_ms(&self) -> u64 {
        self.ttl_ms
    }
    /// Explicit trusted time; this model reads no clock.
    #[must_use]
    pub const fn trusted_now(&self) -> TimestampMillis {
        self.trusted_now
    }
    /// Captured generation, without owner configuration proof.
    #[must_use]
    pub const fn config_version(&self) -> Option<u64> {
        self.config_version
    }
    /// Original installed version owner, not a short projection borrow.
    #[must_use]
    pub const fn policy_version(&self) -> Option<RunningClaimPolicyVersion<'v>> {
        self.policy_version
    }
}
