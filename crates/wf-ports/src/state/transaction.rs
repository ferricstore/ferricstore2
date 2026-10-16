//! Semantic transaction authority. Implementations must keep one exclusive
//! revision through all reads and consumed commit; this module supplies no engine.

use std::fmt;

use wf_domain::claim::apply::ClaimQuery;
use wf_domain::claim::{DueOrder, LaneKey, LaneOrder};
use wf_domain::policy::complete::{CompletePolicyValue, PolicySchemaCharge, PolicySchemaLimits};
use wf_domain::workflow::WorkflowRecord;
use wf_domain::workflow::control::{ControlState, plan_control};
use wf_domain::workflow::indexed::{IndexedBatchPlan, IndexedDecision, OrderingMetadata};
use wf_domain::workflow::request::{RequestScope, RetainedOutcome, RetainedStatus};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};

use crate::log::{RecordBatchApplyDecision, RecordBatchApplyPlan, record_batch_retained_status};

/// Typed core record with its tenant, placement and canonical ordering metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredWorkflowRecord {
    /// Exact tenant within the transaction's group/recovery namespace.
    pub tenant: TenantId,
    /// Actual authoritative storage partition.
    pub partition: PartitionId,
    /// Immutable represented core record.
    pub record: WorkflowRecord,
    /// Priority0..2 and exact u128 logical-state entry sequence.
    pub ordering: OrderingMetadata,
}

/// Operational transaction failure, never retained as a business rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateTransactionError {
    /// The adapter proves the durable predecessor is unchanged.
    KnownAborted,
    /// Commit may be durable; fence service and reconcile after reopen.
    Uncertain,
    /// Stored namespace, record or index authority is inconsistent.
    Corrupt,
    /// The backend cannot currently provide authority.
    Unavailable,
    /// A positive explicit cumulative read or local cursor cap was exhausted.
    Limit,
    /// An admitted host reservation failed.
    Allocation,
    /// The supplied mathematical change container is incoherent.
    InvalidChanges,
}

impl fmt::Display for StateTransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::KnownAborted => "state transaction known aborted",
            Self::Uncertain => "state transaction commit uncertain",
            Self::Corrupt => "corrupt state authority",
            Self::Unavailable => "state authority unavailable",
            Self::Limit => "state transaction read limit",
            Self::Allocation => "state transaction allocation failed",
            Self::InvalidChanges => "invalid state transaction changes",
        })
    }
}
impl std::error::Error for StateTransactionError {}

/// Opens one exclusive ordered group/recovery revision without copying namespace.
pub trait GroupApplyStore {
    /// Backend-private exclusive guard, independent of consensus framework types.
    type Transaction<'s>: GroupApplyTransaction
    where
        Self: 's;

    /// Acquires one revision with positive cumulative step/owned-byte/scratch caps.
    /// Temporary scope/key/value/seek owners and backend finite-bound copies
    /// debit C prospectively without refunds, separately from returned-owner R.
    ///
    /// # Errors
    /// Operational acquisition, namespace or profile failure.
    fn begin_apply<'s>(
        &'s mut self,
        group: &'s GroupId,
        recovery: &'s RecoveryId,
        maximum_read_steps: usize,
        maximum_read_bytes: usize,
        maximum_temporary_bytes: usize,
    ) -> Result<Self::Transaction<'s>, StateTransactionError>;
}

/// One stable authority revision with shared reads and consumed commit.
///
/// All probes/open attempts, misses and physically inspected range candidates
/// count cumulatively, including filtered candidates and empty end probes.
/// Control costs512 logical bytes; copied record/outcome owners cost512 plus ALL
/// owned variable fields; inspected due keys cost128 plus their four byte fields.
/// Materialized filter records cost additionally. Admit before ownership/progress;
/// no refunds/page resets. Backend pages/cache, borrowed donors and caller copies
/// are outside this logical profile. Every returned identity must match its query;
/// wrong rows are Corrupt, never absent. True FIFO heads cannot be projections.
///
/// A live cursor prevents consumed commit:
/// ```compile_fail
/// use wf_ports::state::{GroupApplyChanges, GroupApplyTransaction, GroupDueCursor};
/// use wf_domain::claim::apply::ClaimQuery;
/// use wf_types::identity::TenantId;
/// fn invalid<T: GroupApplyTransaction>(tx: T, tenant: &TenantId,
///     query: ClaimQuery<'_>, changes: &GroupApplyChanges<'_, '_>) {
///     let mut cursor = tx.due_cursor(tenant, query, None, 1, 1024).unwrap();
///     tx.commit(changes).unwrap();
///     let _ = cursor.next();
/// }
/// ```
pub trait GroupApplyTransaction: Sized {
    /// Authentic ordered range cursor borrowing this same transaction.
    type DueCursor<'tx>: GroupDueCursor
    where
        Self: 'tx;
    /// Reads recovery-wide exact partition placement; missing is observed absence.
    /// One probe and512+copied group bytes are charged before ownership.
    /// # Errors
    /// Operational backend, corruption or cumulative admission failure.
    fn placement(&self, partition: &PartitionId) -> Result<Option<GroupId>, StateTransactionError>;
    /// Reads complete normalized V2 backing, never a retry-only persisted shadow.
    /// Pre-admit checked2048+216*wire bytes against R before complete decoding,
    /// including typed metadata and uniqueness scratch. Use this SAME schema
    /// account; validate exact type. Original ETF/foreign/corrupt rows refuse.
    /// # Errors
    /// Operational backend, corruption, schema work or cumulative admission failure.
    fn installed_policy(
        &self,
        tenant: &TenantId,
        workflow_type: &[u8],
        limits: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<CompletePolicyValue>, StateTransactionError>;
    /// Reads global group control; unapplied differs from applied index0.
    /// # Errors
    /// Operational backend, corruption or cumulative admission failure.
    fn control(&self) -> Result<ControlState, StateTransactionError>;
    /// Reads one exact original scoped outcome, including actual absence.
    /// # Errors
    /// Operational backend, corruption or cumulative admission failure.
    fn outcome(
        &self,
        tenant: &TenantId,
        request: &RequestId,
    ) -> Result<Option<RetainedOutcome>, StateTransactionError>;
    /// Observes any retained key in this tenant and transaction group/recovery.
    ///
    /// Under this same exclusive revision, charge one cumulative step BEFORE
    /// opening/seeking its exact physical prefix and another BEFORE observing
    /// the first candidate or actual end. Both true and false need both steps;
    /// refusal retains earlier debits and never establishes absence. A first
    /// key outside the prefix proves emptiness without reading/copying a value.
    /// No outcome owner or 512-byte charge is needed for this observation; owned
    /// seek bounds and backend copies require separate adapter admission.
    /// # Errors
    /// Operational backend, corruption or cumulative admission failure.
    fn retained_nonempty(&self, tenant: &TenantId) -> Result<bool, StateTransactionError>;
    /// Reads one exact authoritative core record, never a whole-group snapshot.
    /// # Errors
    /// Operational backend, corruption or cumulative admission failure.
    fn record(
        &self,
        tenant: &TenantId,
        partition: &PartitionId,
        id: &RecordId,
    ) -> Result<Option<StoredWorkflowRecord>, StateTransactionError>;
    /// Tests exact semantic parallel membership under the same revision.
    /// # Errors
    /// Operational backend, corruption or cumulative admission failure.
    fn parallel_member(
        &self,
        tenant: &TenantId,
        due: DueOrder<'_>,
    ) -> Result<bool, StateTransactionError>;
    /// Reads the exact member's authoritative record in the requested lane.
    /// # Errors
    /// Operational backend, corruption or cumulative admission failure.
    fn fifo_member(
        &self,
        tenant: &TenantId,
        lane: LaneKey<'_>,
        member: LaneOrder<'_>,
    ) -> Result<Option<StoredWorkflowRecord>, StateTransactionError>;
    /// Reads the actual first member, including a running/unscheduled blocker.
    /// # Errors
    /// Operational backend, corruption or cumulative admission failure.
    fn fifo_head(
        &self,
        tenant: &TenantId,
        lane: LaneKey<'_>,
    ) -> Result<Option<StoredWorkflowRecord>, StateTransactionError>;
    /// Opens a borrowed-filter authentic prefix with positive local caps.
    /// # Errors
    /// Operational backend, corruption or cumulative admission failure.
    fn due_cursor<'tx>(
        &'tx self,
        tenant: &'tx TenantId,
        query: ClaimQuery<'tx>,
        after: Option<DueOrder<'tx>>,
        maximum_rows: usize,
        maximum_bytes: usize,
    ) -> Result<Self::DueCursor<'tx>, StateTransactionError>;
    /// Atomically persists exactly these validated changes, consuming authority.
    ///
    /// Compare group/recovery, exact expected control/stamp and affected versions
    /// against THIS guard. Accepted includes records/order/index/history/outcome;
    /// rejection only outcome/control; replay/conflict/expired only control;
    /// exact `LogReplay` nothing. Publish only after proved durable success.
    /// # Errors
    /// `KnownAborted` proves unchanged predecessor; Uncertain requires fencing and
    /// durable reopen/reconciliation, never blind retry or business retention.
    fn commit(self, changes: &GroupApplyChanges<'_, '_>) -> Result<(), StateTransactionError>;
}

/// A borrowed range row or a truthful distinction between exhaustion and budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DueCursorStep<'row> {
    /// Fully admitted consumed semantic row.
    Row(DueOrder<'row>),
    /// Actual requested range exhausted under this same transaction.
    RangeEnd,
    /// Admission/returned-row cap ended traversal; no emptiness proof.
    BudgetEnd,
}

/// A bounded cursor; filtered rows advance only after FULL consumption/admission.
///
/// A returned row prevents advance while its borrow is used:
/// ```compile_fail
/// use wf_ports::state::{DueCursorStep, GroupDueCursor};
/// fn invalid<C: GroupDueCursor>(cursor: &mut C) {
///     let step = cursor.next().unwrap();
///     let _ = cursor.next();
///     if let DueCursorStep::Row(row) = step { let _ = row.id; }
/// }
/// ```
pub trait GroupDueCursor {
    /// Inspects a bounded ordered prefix; latched terminal calls do no new work.
    /// # Errors
    /// Operational corruption/backend failures; budget exhaustion is `BudgetEnd`.
    fn next(&mut self) -> Result<DueCursorStep<'_>, StateTransactionError>;
    /// Last fully consumed inspected key, INCLUDING filtered-out candidates.
    /// Never advances past incomplete admission; continuation is advisory and is
    /// freshly validated on reopen, without resetting the transaction's budget.
    fn position(&self) -> Option<DueOrder<'_>>;
}

/// Validated mathematical changes, with no origin or persistence certificate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupApplyChanges<'scope, 'view> {
    scope: &'scope RequestScope,
    expected: ControlState,
    plan: RecordBatchApplyPlan<'view>,
}

impl<'scope, 'view> GroupApplyChanges<'scope, 'view> {
    /// Rechecks predecessor/control relation, scope and Fresh status/body.
    /// # Errors
    /// Rejects incoherent supplied changes without publishing any state.
    pub fn new(
        scope: &'scope RequestScope,
        expected: ControlState,
        plan: RecordBatchApplyPlan<'view>,
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
            || control.is_log_replay()
                != matches!(plan.decision(), RecordBatchApplyDecision::LogReplay)
        {
            return Err(StateTransactionError::InvalidChanges);
        }
        match plan.decision() {
            RecordBatchApplyDecision::Fresh { operation, outcome } => {
                let status = match operation {
                    IndexedDecision::Accepted(_) => RetainedStatus::Accepted,
                    IndexedDecision::Rejected(reason) => RetainedStatus::IndexedRejected(*reason),
                };
                if outcome.scope() != scope
                    || outcome.status() != status
                    || record_batch_retained_status(
                        outcome.intent(),
                        outcome.response_version(),
                        outcome.response(),
                    )
                    .map_err(|_| StateTransactionError::InvalidChanges)?
                        != status
                {
                    return Err(StateTransactionError::InvalidChanges);
                }
            }
            RecordBatchApplyDecision::Replay(outcome) if outcome.scope() != scope => {
                return Err(StateTransactionError::InvalidChanges);
            }
            _ => {}
        }
        Ok(Self {
            scope,
            expected,
            plan,
        })
    }
    /// Reads tenant business scope; control remains global group/recovery.
    #[must_use]
    pub const fn scope(&self) -> &'scope RequestScope {
        self.scope
    }
    /// Reads the exact predecessor the backend must compare in its guard.
    #[must_use]
    pub const fn expected(&self) -> ControlState {
        self.expected
    }
    /// Reads complete decision classification without changing ownership.
    #[must_use]
    pub const fn plan(&self) -> &RecordBatchApplyPlan<'view> {
        &self.plan
    }
    /// Moves the existing plan after the caller has completed consumed commit.
    /// This mathematical extraction grants no durability or origin certificate.
    #[must_use]
    pub fn into_plan(self) -> RecordBatchApplyPlan<'view> {
        self.plan
    }
    /// Accepted touched-only business/index/history plan; absent for rejection.
    #[must_use]
    pub fn business(&self) -> Option<&IndexedBatchPlan<'view>> {
        match self.plan.decision() {
            RecordBatchApplyDecision::Fresh {
                operation: IndexedDecision::Accepted(plan),
                ..
            } => Some(plan),
            _ => None,
        }
    }
    /// New outcome only for Fresh; original replay/conflict owners stay unchanged.
    #[must_use]
    pub fn outcome(&self) -> Option<&RetainedOutcome> {
        match self.plan.decision() {
            RecordBatchApplyDecision::Fresh { outcome, .. } => Some(outcome),
            _ => None,
        }
    }
    /// Next global control unless this is an exact last-log no-op.
    #[must_use]
    pub fn control(&self) -> Option<ControlState> {
        (!self.plan.control().is_log_replay()).then_some(self.plan.control().next())
    }
}
