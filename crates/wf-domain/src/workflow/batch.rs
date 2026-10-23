//! Atomic decisions over bounded touched records in one authoritative group.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use wf_types::identity::{GroupId, PartitionId};

use crate::claim::ScopedRecordId;
use crate::policy::InstalledRetryPolicy;

use super::admission::{
    AdmissionError, AdmissionLimits, add, admit_command, record_bytes, start_and_claim_bytes,
    step_continue_bytes,
};
use super::{
    CommittedCommand, DomainError, Operation, TransitionPlan, WorkflowRecord, reduce_with_policy,
};

mod authority;
mod independent;
pub(super) use authority::{BatchAuthority, observed_current};
pub use independent::{
    IndependentBatchDecision, IndependentBatchPlan, IndependentTransitionDecision,
    IndependentTransitionPlan, decide_independent_terminal_batch,
    decide_independent_transition_batch,
};
pub(super) use independent::{
    IndependentProfile, IndependentResults, IndependentStep, charge as charge_independent,
    preflight as independent_preflight, run as run_independent,
};

/// Borrowed group-local authoritative placement/current-record view.
///
/// The apply owner must supply complete canonical relevant records and placement,
/// serialize access, and persist the returned plans atomically. A projection or
/// client routing hint cannot supply either authority. No durable apply occurs.
#[derive(Clone, Copy, Debug)]
pub struct GroupView<'a> {
    /// Group whose coherent transaction the view represents.
    pub group: &'a GroupId,
    /// Authoritative partition-to-group placement, with no live migration.
    pub placements: &'a BTreeMap<PartitionId, GroupId>,
    /// Unique canonical record identity in each represented partition.
    pub records: &'a BTreeMap<ScopedRecordId<'a>, &'a WorkflowRecord>,
}

/// One immutable prepared command and its actual encoding inputs.
#[derive(Clone, Copy, Debug)]
pub struct BatchItem<'a> {
    /// Logical partition whose authoritative placement is revalidated.
    pub partition: &'a PartitionId,
    /// Explicit committed single-record command.
    pub command: &'a CommittedCommand,
    /// Actual encoded state key; decoder verifies partition/ID correspondence.
    pub state_key: &'a [u8],
    /// Actual encoded values, verified against command fields by the decoder.
    pub encoded_values: &'a [&'a [u8]],
    /// Current coherent authoritative retry-only policy, if needed.
    pub retry_policy: Option<&'a InstalledRetryPolicy>,
}

impl<'a> BatchItem<'a> {
    /// Builds a typed-only item; codec values can be attached before admission.
    #[must_use]
    pub const fn new(
        partition: &'a PartitionId,
        command: &'a CommittedCommand,
        state_key: &'a [u8],
    ) -> Self {
        Self {
            partition,
            command,
            state_key,
            encoded_values: &[],
            retry_policy: None,
        }
    }
}

/// Only one record's plan and scope, preserving the input order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScopedPlan {
    partition: PartitionId,
    plan: TransitionPlan,
}

impl ScopedPlan {
    /// Borrows the logical partition to be updated atomically.
    #[must_use]
    pub const fn partition(&self) -> &PartitionId {
        &self.partition
    }
    /// Borrows the proposed record/history mutation or legacy no-op.
    #[must_use]
    pub const fn plan(&self) -> &TransitionPlan {
        &self.plan
    }
    /// Transfers the bounded values to a future atomic apply adapter.
    #[must_use]
    pub fn into_parts(self) -> (PartitionId, TransitionPlan) {
        (self.partition, self.plan)
    }
}

/// A complete atomic write description containing only touched records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchPlan {
    items: Vec<ScopedPlan>,
}

impl BatchPlan {
    /// Borrows all command results in original order.
    #[must_use]
    pub fn items(&self) -> &[ScopedPlan] {
        &self.items
    }
    /// Transfers all plans without cloning a group or partition.
    #[must_use]
    pub fn into_items(self) -> Vec<ScopedPlan> {
        self.items
    }
}

/// Complete accepted plan or deterministic rejection with no partial writes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BatchDecision {
    /// Every represented member validated.
    Accepted(BatchPlan),
    /// No record/history plan is exposed.
    Rejected(BatchRejection),
}

/// Deterministic command failure, independent of local resources.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchRejection {
    /// One member violates a domain guard or source command rule.
    Domain(DomainError),
    /// Actual command data exceeds immutable replicated limits.
    Admission(AdmissionError),
    /// Atomic source bulk forbids repeating a record ID, even across partitions.
    DuplicateId,
    /// Authoritative placement maps a requested partition to another group.
    CrossGroup,
}

/// Host/authority failure: never retain this as a workflow business rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplyError {
    /// Authoritative state/placement is missing, inconsistent or violates bounds.
    InvalidAuthority,
    /// A fallible bounded output allocation failed; recover/retry this entry.
    AllocationFailed,
    /// A committed position or identity conflicts with the coherent apply view.
    PositionConflict,
}

impl fmt::Display for ApplyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidAuthority => "invalid authoritative workflow view",
            Self::AllocationFailed => "workflow apply allocation failed",
            Self::PositionConflict => "committed workflow position conflicts with apply view",
        })
    }
}
impl std::error::Error for ApplyError {}

/// Decides an ordered atomic same-group batch using only borrowed canonical state.
///
/// Checks placement, unique IDs, actual bytes and existing-record bytes before
/// copying any record. Member rejection discards every provisional plan. Duplicate
/// source IDs mean this bulk API cannot encode sequential operations on one record;
/// independent/sequential pipeline semantics require a separate command contract.
///
/// # Errors
/// Returns operational authority/allocation failure; these must not be persisted
/// as deterministic rejections or advance a durable applied checkpoint.
pub fn decide_group_batch(
    view: GroupView<'_>,
    items: &[BatchItem<'_>],
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<BatchDecision, ApplyError> {
    let bytes = match admitted_bytes(view, items, encoded_entry, limits)? {
        Ok(bytes) => bytes,
        Err(rejection) => return Ok(BatchDecision::Rejected(rejection)),
    };
    if let Err(error) = limits.charge(bytes, items.len()) {
        return Ok(BatchDecision::Rejected(BatchRejection::Admission(error)));
    }
    build_plan(view, items)
}

pub(super) fn build_plan(
    view: GroupView<'_>,
    items: &[BatchItem<'_>],
) -> Result<BatchDecision, ApplyError> {
    build_plan_with(BatchAuthority::Full(view), items)
}

pub(super) fn build_plan_with(
    view: BatchAuthority<'_, '_>,
    items: &[BatchItem<'_>],
) -> Result<BatchDecision, ApplyError> {
    let mut plans = Vec::new();
    plans
        .try_reserve_exact(items.len())
        .map_err(|_| ApplyError::AllocationFailed)?;
    for item in items {
        let record = view.record(identity(*item))?;
        let retry_policy = view.retry_policy(*item, record)?;
        let plan = match reduce_with_policy(record, item.command, retry_policy) {
            Ok(plan) => plan,
            Err(error) => return Ok(BatchDecision::Rejected(BatchRejection::Domain(error))),
        };
        plans.push(ScopedPlan {
            partition: item.partition.clone(),
            plan,
        });
    }
    Ok(BatchDecision::Accepted(BatchPlan { items: plans }))
}

pub(super) fn admitted_bytes(
    view: GroupView<'_>,
    items: &[BatchItem<'_>],
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<Result<usize, BatchRejection>, ApplyError> {
    admitted_with(BatchAuthority::Full(view), items, encoded_entry, limits)
}

pub(super) fn admitted_with(
    view: BatchAuthority<'_, '_>,
    items: &[BatchItem<'_>],
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<Result<usize, BatchRejection>, ApplyError> {
    admitted_with_identity(view, items, encoded_entry, limits, |item| {
        Ok(&item.command.id)
    })
}

fn admitted_with_identity<'input, K: Ord>(
    view: BatchAuthority<'_, '_>,
    items: &[BatchItem<'input>],
    encoded_entry: &[u8],
    limits: AdmissionLimits,
    project: impl Fn(BatchItem<'input>) -> Result<K, ApplyError>,
) -> Result<Result<usize, BatchRejection>, ApplyError> {
    if items.is_empty() || items.len() > limits.batch_items() {
        return Ok(Err(BatchRejection::Admission(AdmissionError::BatchItems)));
    }
    if let Err(error) = limits.check_total(encoded_entry.len()) {
        return Ok(Err(BatchRejection::Admission(error)));
    }
    let mut ids = BTreeSet::new();
    let mut total = 0;
    for item in items {
        let Some(group) = view.placement(item.partition) else {
            return Err(ApplyError::InvalidAuthority);
        };
        if group != view.group() {
            return Ok(Err(BatchRejection::CrossGroup));
        }
        if !ids.insert(project(*item)?) {
            return Ok(Err(BatchRejection::DuplicateId));
        }
        let admitted =
            match admit_command(item.command, item.state_key, item.encoded_values, limits) {
                Ok(bytes) => bytes,
                Err(error) => return Ok(Err(BatchRejection::Admission(error))),
            };
        if let Err(error) = add(&mut total, admitted)
            .and_then(|()| add(&mut total, item.partition.as_bytes().len()))
            .and_then(|()| limits.check_total(total))
        {
            return Ok(Err(BatchRejection::Admission(error)));
        }
        let record = view.record(identity(*item))?;
        if let Some(record) = record {
            if record.id() != &item.command.id {
                return Err(ApplyError::InvalidAuthority);
            }
            let bytes = record_bytes(record, limits).map_err(|_| ApplyError::InvalidAuthority)?;
            add(&mut total, bytes).map_err(|_| ApplyError::InvalidAuthority)?;
            if matches!(item.command.operation, Operation::RetryWithPolicy(_))
                && let Some(policy) = view.retry_policy(*item, Some(record))?
            {
                admit_retry_policy(record, policy, limits, &mut total)?;
            }
            if let Err(error) = limits.check_total(total) {
                return Ok(Err(BatchRejection::Admission(error)));
            }
            if let Err(error) = step_continue_bytes(record, item.command, limits)
                .and_then(|bytes| add(&mut total, bytes))
                .and_then(|()| limits.check_total(total))
            {
                return Ok(Err(BatchRejection::Admission(error)));
            }
        }
        if let Err(error) = start_and_claim_bytes(record, item.command, limits)
            .and_then(|bytes| add(&mut total, bytes))
            .and_then(|()| limits.check_total(total))
        {
            return Ok(Err(BatchRejection::Admission(error)));
        }
    }
    Ok(Ok(total))
}

pub(super) fn admit_retry_policy(
    record: &WorkflowRecord,
    policy: &InstalledRetryPolicy,
    limits: AdmissionLimits,
    total: &mut usize,
) -> Result<(), ApplyError> {
    if policy.workflow_type() != record.workflow_type() {
        return Err(ApplyError::InvalidAuthority);
    }
    let state = super::validation::retry_run_state(record);
    for layer in [Some(policy.type_retry()), policy.state_retry(state)]
        .into_iter()
        .flatten()
    {
        if let Some(target) = &layer.exhausted_to {
            limits
                .check_value(target)
                .map_err(|_| ApplyError::InvalidAuthority)?;
            add(total, target.len()).map_err(|_| ApplyError::InvalidAuthority)?;
        }
    }
    Ok(())
}

pub(super) fn admitted_claim_bytes(
    view: GroupView<'_>,
    items: &[BatchItem<'_>],
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<Result<usize, BatchRejection>, ApplyError> {
    admitted_claim_with(BatchAuthority::Full(view), items, encoded_entry, limits)
}

pub(super) fn admitted_claim_with(
    view: BatchAuthority<'_, '_>,
    items: &[BatchItem<'_>],
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<Result<usize, BatchRejection>, ApplyError> {
    match admitted_with_identity(view, items, encoded_entry, limits, |item| {
        if !matches!(
            item.command.operation,
            Operation::Claim(_) | Operation::Reclaim(_)
        ) {
            return Err(ApplyError::InvalidAuthority);
        }
        Ok(identity(item))
    })? {
        Err(BatchRejection::DuplicateId | BatchRejection::CrossGroup) => {
            Err(ApplyError::InvalidAuthority)
        }
        other => Ok(other),
    }
}

fn identity(item: BatchItem<'_>) -> ScopedRecordId<'_> {
    ScopedRecordId {
        partition_key: item.partition.as_bytes(),
        id: item.command.id.as_bytes(),
    }
}
