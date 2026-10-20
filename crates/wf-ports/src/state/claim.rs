//! Claim reads and consumed publication on the existing exclusive group guard.

use wf_domain::claim::apply::{ClaimDecision, ClaimQuery};
use wf_domain::claim::{DueOrder, LaneKey};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlState, plan_control};
use wf_domain::workflow::indexed::IndexedBatchPlan;
use wf_domain::workflow::request::{RequestScope, RetainedOutcome, RetainedStatus};
use wf_types::identity::TenantId;
use wf_types::version::TimestampMillis;

use crate::log::{
    ClaimApplyDecision, ClaimApplyPlan, CodecError, claim_response_matches, claim_retained_summary,
};

use super::{GroupApplyTransaction, StateTransactionError};

/// Physical due row tags; a FIFO head is never a second parallel candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimDueKind {
    /// Canonical parallel-mode due membership.
    Parallel,
    /// The actual first FIFO lane member's due representation.
    FifoHead,
}

/// A borrowed admitted row, truthful exhaustion or operational budget end.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimDueStep<'row> {
    /// Actual row with its validated physical kind.
    Row {
        /// Complete semantic due identity.
        due: DueOrder<'row>,
        /// Parallel or already represented true FIFO head.
        kind: ClaimDueKind,
    },
    /// The requested relevant physical stream actually ended.
    RangeEnd,
    /// Operational steps/bytes ended traversal; no emptiness proof is supplied.
    BudgetEnd,
}

/// A distinct actual lane, truthful exhaustion or operational budget end.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimLaneStep<'row> {
    /// Actual nonempty relevant lane, without traversing its followers.
    Lane(LaneKey<'row>),
    /// The requested lane namespace actually ended.
    RangeEnd,
    /// Operational exhaustion; this is not a semantic head-budget result.
    BudgetEnd,
}

/// Same-guard physical readers and consumed Claim publication.
pub trait GroupClaimTransaction: GroupApplyTransaction {
    /// Borrowed due reader; keeping it live prevents consuming the guard.
    type ClaimDueCursor<'tx>: GroupClaimDueCursor
    where
        Self: 'tx;
    /// Borrowed distinct-lane reader under that same guard.
    type ClaimLaneCursor<'tx>: GroupClaimLaneCursor
    where
        Self: 'tx;

    /// Opens a query-scoped due stream at effective committed time.
    /// # Errors
    /// Reports actual operational namespace/profile/backend failures.
    fn claim_due_cursor<'tx>(
        &'tx self,
        tenant: &'tx TenantId,
        query: ClaimQuery<'tx>,
        now: TimestampMillis,
    ) -> Result<Self::ClaimDueCursor<'tx>, StateTransactionError>;
    /// Opens a distinct nonempty lane stream, skipping whole follower prefixes.
    /// # Errors
    /// Reports actual operational namespace/profile/backend failures.
    fn claim_lane_cursor<'tx>(
        &'tx self,
        tenant: &'tx TenantId,
        query: ClaimQuery<'tx>,
    ) -> Result<Self::ClaimLaneCursor<'tx>, StateTransactionError>;
    /// Reads the exact physical tag for a reached complete due identity.
    /// # Errors
    /// Rejects inconsistent physical values and operational exhaustion.
    fn claim_due_kind(
        &self,
        tenant: &TenantId,
        due: DueOrder<'_>,
    ) -> Result<Option<ClaimDueKind>, StateTransactionError>;
    /// Rechecks this guard's actual profile/authority and publishes atomically.
    /// # Errors
    /// `KnownAborted` proves no publication; `Uncertain` requires fencing/reopen.
    fn commit_claim(self, changes: &GroupClaimChanges<'_, '_>)
    -> Result<(), StateTransactionError>;
}

/// A latched borrowed due cursor with no owned whole-state snapshot.
pub trait GroupClaimDueCursor {
    /// Advances only after prospective operational admission.
    /// # Errors
    /// Reports corruption/backend errors; operational exhaustion is `BudgetEnd`.
    fn next(&mut self) -> Result<ClaimDueStep<'_>, StateTransactionError>;
    /// Last fully consumed inspected identity, including filtered physical rows.
    fn position(&self) -> Option<DueOrder<'_>>;
}

/// A latched distinct-lane cursor; returned borrows prevent advance.
pub trait GroupClaimLaneCursor {
    /// Advances by a complete escaped lane-prefix successor.
    /// # Errors
    /// Reports corruption/backend errors; operational exhaustion is `BudgetEnd`.
    fn next(&mut self) -> Result<ClaimLaneStep<'_>, StateTransactionError>;
    /// Last fully consumed lane; incomplete admission never advances it.
    fn position(&self) -> Option<LaneKey<'_>>;
}

/// Validated mathematical changes, without origin or durability certification.
#[derive(Debug)]
pub struct GroupClaimChanges<'scope, 'current> {
    scope: &'scope RequestScope,
    expected: ControlState,
    plan: ClaimApplyPlan<'current>,
}

impl<'scope, 'current> GroupClaimChanges<'scope, 'current> {
    /// Checks predecessor, scope, original metadata and complete receipt correspondence.
    /// The backend must repeat validation with its captured actual profile.
    /// # Errors
    /// Returns `Limit` for narrower profiles and `InvalidChanges` for incoherent proposals.
    pub fn new(
        scope: &'scope RequestScope,
        expected: ControlState,
        plan: ClaimApplyPlan<'current>,
        admission: AdmissionLimits,
    ) -> Result<Self, StateTransactionError> {
        let control = plan.control();
        let next = control.next();
        let identity = next
            .last_entry()
            .ok_or(StateTransactionError::InvalidChanges)?;
        let stamp = next
            .last_entry_stamp()
            .ok_or(StateTransactionError::InvalidChanges)?;
        if plan_control(expected, identity, stamp)
            .map_err(|_| StateTransactionError::InvalidChanges)?
            != control
            || control.is_log_replay() != matches!(plan.decision(), ClaimApplyDecision::LogReplay)
        {
            return Err(StateTransactionError::InvalidChanges);
        }
        match plan.decision() {
            ClaimApplyDecision::Fresh { operation, outcome } => {
                validate_outcome(scope, outcome, admission)?;
                let status = match operation {
                    ClaimDecision::Accepted(_) => RetainedStatus::Accepted,
                    ClaimDecision::Rejected(reason) => RetainedStatus::ClaimRejected(*reason),
                };
                if outcome.status() != status
                    || outcome.original_position() != identity.log()
                    || outcome.original_time() != control.execution_time()
                    || !claim_response_matches(
                        operation,
                        outcome.response(),
                        outcome.original_time(),
                        admission,
                    )
                    .map_err(codec_error)?
                {
                    return Err(StateTransactionError::InvalidChanges);
                }
            }
            ClaimApplyDecision::Replay(outcome) => validate_outcome(scope, outcome, admission)?,
            _ => {}
        }
        Ok(Self {
            scope,
            expected,
            plan,
        })
    }
    /// Reads the tenant and group/recovery namespace.
    #[must_use]
    pub const fn scope(&self) -> &'scope RequestScope {
        self.scope
    }
    /// Reads the exact predecessor to compare in the consumed guard.
    #[must_use]
    pub const fn expected(&self) -> ControlState {
        self.expected
    }
    /// Borrows the complete mathematical classification.
    #[must_use]
    pub const fn plan(&self) -> &ClaimApplyPlan<'current> {
        &self.plan
    }
    /// Moves the existing plan without granting publication authority.
    #[must_use]
    pub fn into_plan(self) -> ClaimApplyPlan<'current> {
        self.plan
    }
    /// Accepted touched record/index/history writes; empty/rejected have none.
    #[must_use]
    pub fn business(&self) -> Option<&IndexedBatchPlan<'current>> {
        match self.plan.decision() {
            ClaimApplyDecision::Fresh {
                operation: ClaimDecision::Accepted(plan),
                ..
            } => plan.batch(),
            _ => None,
        }
    }
    /// New immutable receipt only for Fresh.
    #[must_use]
    pub fn outcome(&self) -> Option<&RetainedOutcome> {
        match self.plan.decision() {
            ClaimApplyDecision::Fresh { outcome, .. } => Some(outcome),
            _ => None,
        }
    }
    /// Proposed processing control except for exact last-log replay.
    #[must_use]
    pub fn control(&self) -> Option<ControlState> {
        (!self.plan.control().is_log_replay()).then_some(self.plan.control().next())
    }
}

fn validate_outcome(
    scope: &RequestScope,
    outcome: &RetainedOutcome,
    admission: AdmissionLimits,
) -> Result<(), StateTransactionError> {
    if outcome.scope() != scope {
        return Err(StateTransactionError::InvalidChanges);
    }
    let (status, _) = claim_retained_summary(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
        outcome.original_time(),
        admission,
    )
    .map_err(codec_error)?;
    if outcome.status() != status {
        return Err(StateTransactionError::InvalidChanges);
    }
    Ok(())
}

fn codec_error(error: CodecError) -> StateTransactionError {
    match error {
        CodecError::ByteLimit | CodecError::ItemLimit | CodecError::ValueLimit => {
            StateTransactionError::Limit
        }
        _ => StateTransactionError::InvalidChanges,
    }
}
