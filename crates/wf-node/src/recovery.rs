//! Bounded startup audit/replay on separately owned product databases.
//!
//! These compositions neither start a service nor supply consensus commitment.
//! They process only the already durable committed prefix.

use wf_domain::claim::apply::ClaimWorkLimits;
use wf_ports::consensus::{RaftMembershipError, RaftMembershipLimits};
use wf_ports::log::{CodecError, RaftLogError, RaftLogStore};
use wf_ports::state::StateTransactionError;
use wf_storage::log::RedbLogStore;
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    ClaimTransactionError, CorePolicyTransactionError, OriginalPolicyTransactionError,
    RecordBatchTransactionError, RecordBatchTransactionLimits, WorkflowValueTransactionError,
};
use wf_types::identity::{GroupId, RecoveryId};

mod catalog_ack;
mod catalog_membership;
mod catalog_progress;
mod catalog_prune;
mod error;
pub use error::CommittedRecoveryError;
mod credit;
mod join;
mod original_policy;
mod replay;
mod snapshot;
mod workflow_values;

pub use snapshot::{
    SnapshotRecoveryError, install_received_snapshot, purge_snapshot, recover_snapshotted,
    restore_received_snapshot,
};

/// Explicit per-guard profiles and whole-call startup work/owner ceilings.
/// No default semantic TTL or cluster apply profile is manufactured.
#[derive(Clone, Copy)]
pub struct CommittedRecoveryLimits {
    membership: RaftMembershipLimits,
    state: (usize, usize, usize, usize),
    normal: RecordBatchTransactionLimits,
    claim_work: Option<ClaimWorkLimits>,
    value_claim_work: Option<ClaimWorkLimits>,
    core_policy: bool,
    original_policy: bool,
    workflow_values: Option<[usize; 4]>,
    stored_workflow_values: bool,
    step_continue: workflow_values::StepRecoveryProfile,
    credit: Option<credit::CreditRecoveryProfile>,
    governance_catalog: catalog_membership::GovernanceCatalogRecoveryProfile,
    work: (usize, usize),
    entry: usize,
    workspace: usize,
}

impl CommittedRecoveryLimits {
    /// Sets membership, state(steps,observationR,controlR,C), Normal, work(points,
    /// copied body bytes), point-owner J and total W profiles.
    /// # Errors
    /// Refuses zero required fields or a point cap below its fixed512 fee.
    pub fn new(
        membership: RaftMembershipLimits,
        state: (usize, usize, usize, usize),
        normal: RecordBatchTransactionLimits,
        work: (usize, usize),
        maximum_owned_entry_bytes: usize,
        workspace_bytes: usize,
    ) -> Result<Self, CommittedRecoveryError> {
        if [
            state.0,
            state.1,
            state.2,
            state.3,
            work.0,
            work.1,
            workspace_bytes,
            normal.maximum_read_steps,
            normal.maximum_read_bytes,
            normal.maximum_staging_bytes,
            normal.maximum_adapter_bytes,
            normal.workspace_bytes,
        ]
        .contains(&0)
            || maximum_owned_entry_bytes < 512
            || normal.outcome_ttl_ms == 0
        {
            return Err(CommittedRecoveryError::InvalidLimits);
        }
        Ok(Self {
            membership,
            state,
            normal,
            claim_work: None,
            value_claim_work: None,
            core_policy: false,
            original_policy: false,
            workflow_values: None,
            stored_workflow_values: false,
            step_continue: workflow_values::StepRecoveryProfile::new(false),
            credit: None,
            governance_catalog: catalog_membership::GovernanceCatalogRecoveryProfile::Disabled,
            work,
            entry: maximum_owned_entry_bytes,
            workspace: workspace_bytes,
        })
    }
    /// Explicitly enables physical Claim replay with its immutable work profile.
    #[must_use]
    pub const fn with_claim_work_limits(mut self, work: ClaimWorkLimits) -> Self {
        self.claim_work = Some(work);
        self
    }
    /// Reads the selected Claim profile; legacy construction leaves it absent.
    #[must_use]
    pub const fn claim_work_limits(self) -> Option<ClaimWorkLimits> {
        self.claim_work
    }
    /// Explicitly enables complete core-policy Patch replay on the normal profile.
    #[must_use]
    pub const fn with_core_policy_patches(mut self) -> Self {
        self.core_policy = true;
        self
    }
    /// Reads the explicit policy opt-in; legacy construction leaves it disabled.
    #[must_use]
    pub const fn core_policy_patches(self) -> bool {
        self.core_policy
    }
    /// Supported membership validation profile.
    #[must_use]
    pub const fn membership_limits(self) -> RaftMembershipLimits {
        self.membership
    }
    /// Cumulative steps within each observation/control guard.
    #[must_use]
    pub const fn maximum_state_read_steps(self) -> usize {
        self.state.0
    }
    /// Cumulative R within each initial/final coherent observation.
    #[must_use]
    pub const fn maximum_applied_state_bytes(self) -> usize {
        self.state.1
    }
    /// Cumulative R within each consumed control-entry application.
    #[must_use]
    pub const fn maximum_control_apply_bytes(self) -> usize {
        self.state.2
    }
    /// Cumulative C within each observation/control guard.
    #[must_use]
    pub const fn maximum_temporary_bytes(self) -> usize {
        self.state.3
    }
    /// Exact existing Normal driver profile, including its explicit semantic TTL.
    #[must_use]
    pub const fn normal_limits(self) -> RecordBatchTransactionLimits {
        self.normal
    }
    /// Whole-call point allowance across audit AND replay.
    #[must_use]
    pub const fn maximum_log_points(self) -> usize {
        self.work.0
    }
    /// Whole-call copied BODY bytes, excluding fixed log-wire headers.
    #[must_use]
    pub const fn maximum_log_body_bytes(self) -> usize {
        self.work.1
    }
    /// Maximum copied point owner512+E.
    #[must_use]
    pub const fn maximum_owned_entry_bytes(self) -> usize {
        self.entry
    }
    /// Simultaneous logical owner ceiling, separate from engine pages/cache.
    #[must_use]
    pub const fn workspace_bytes(self) -> usize {
        self.workspace
    }

    fn base(group: &GroupId, recovery: &RecoveryId) -> Result<usize, CommittedRecoveryError> {
        total(&[
            512,
            group.as_bytes().len(),
            recovery.as_bytes().len(),
            512,
            group.as_bytes().len(),
            recovery.as_bytes().len(),
            2048,
            512,
            std::mem::size_of::<Option<[usize; 4]>>(),
            std::mem::size_of::<CommittedRecoveryError>()
                .max(std::mem::size_of::<SnapshotRecoveryError>()),
        ])
    }
    fn additional_profile_bytes(&self) -> Result<usize, CommittedRecoveryError> {
        let mut owned = 0;
        if self.step_continue.enabled() || self.step_continue.stored() {
            owned = total(&[
                owned,
                std::mem::size_of::<workflow_values::StepRecoveryProfile>(),
            ])?;
        }
        if self.value_claim_work.is_some() {
            owned = total(&[owned, std::mem::size_of::<Option<ClaimWorkLimits>>()])?;
        }
        if self.credit.is_some() {
            owned = total(&[
                owned,
                std::mem::size_of::<Option<credit::CreditRecoveryProfile>>(),
            ])?;
        }
        Ok(owned)
    }
    fn admit(&self, parts: &[usize]) -> Result<(), CommittedRecoveryError> {
        let owned = total(&[total(parts)?, self.additional_profile_bytes()?])?;
        if owned > self.workspace {
            return Err(CommittedRecoveryError::Limit);
        }
        Ok(())
    }
}

fn total(parts: &[usize]) -> Result<usize, CommittedRecoveryError> {
    parts
        .iter()
        .copied()
        .try_fold(0_usize, usize::checked_add)
        .ok_or(CommittedRecoveryError::Limit)
}

/// Audits the full retained history, then applies only the fixed committed suffix.
/// All errors consume both owners; any already committed replay prefix remains
/// durable for explicit reopen. Success is startup consistency, not a Raft service.
/// # Errors
/// Refuses profiles, unsupported/inconsistent bodies, operational errors or budgets.
pub fn recover_committed(
    mut log: RedbLogStore,
    mut state: RedbStateStore,
    group: &GroupId,
    recovery: &RecoveryId,
    limits: CommittedRecoveryLimits,
) -> Result<(RedbLogStore, RedbStateStore), CommittedRecoveryError> {
    use CommittedRecoveryError as Error;
    let limits = &limits;
    if log.group() != group
        || log.recovery() != recovery
        || state.group() != group
        || state.recovery() != recovery
    {
        return Err(Error::NamespaceMismatch);
    }
    let base = CommittedRecoveryLimits::base(group, recovery)?;
    let mut saved = None;
    let initial = join::observe(&mut state, (group, recovery), limits, base, &mut saved)?;
    let metadata = log.state().map_err(Error::Log)?;
    if metadata.last_purged().is_some() {
        return Err(Error::IncompatibleCheckpoint);
    }
    let stop = join::boundary(initial, metadata.committed(), limits)?;
    let audit_end = initial
        .last_entry()
        .map_or(0, |entry| u128::from(entry.log().index().get()) + 1);
    let mut work = [0, limits.work.1];
    let mut latest = None;
    for position in 0..audit_end {
        let index = u64::try_from(position).map_err(|_| Error::Limit)?;
        let entry = join::point(&mut log, index, limits, base, &mut work)?;
        join::audit(
            &entry,
            (group, recovery),
            limits,
            base,
            initial,
            &mut latest,
        )?;
    }
    if latest != saved {
        return Err(Error::MembershipMismatch);
    }
    let mut progress = initial;
    for position in audit_end..stop {
        let index = u64::try_from(position).map_err(|_| Error::Limit)?;
        let entry = join::point(&mut log, index, limits, base, &mut work)?;
        if position + 1 == stop && Some(entry.id()) != metadata.committed() {
            return Err(Error::IdentityMismatch);
        }
        progress = replay::apply(
            &mut state,
            &entry,
            (group, recovery),
            limits,
            base,
            progress,
        )?;
        if entry.body().starts_with(b"FWMB") {
            latest = Some((
                progress.last_entry().ok_or(Error::ProgressMismatch)?,
                progress.last_entry_stamp().ok_or(Error::ProgressMismatch)?,
            ));
        }
    }
    let mut final_membership = None;
    let observed = join::observe(
        &mut state,
        (group, recovery),
        limits,
        base,
        &mut final_membership,
    )?;
    if observed != progress {
        return Err(Error::ProgressMismatch);
    }
    if final_membership != latest {
        return Err(Error::MembershipMismatch);
    }
    if log.state().map_err(Error::Log)?.committed() != metadata.committed() {
        return Err(Error::IncompatibleCheckpoint);
    }
    Ok((log, state))
}
