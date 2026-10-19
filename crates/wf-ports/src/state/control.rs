//! Borrowed Blank/Membership plans and the same-guard atomic capability.

use super::{GroupApplyTransaction, StateTransactionError};
use crate::consensus::{AppliedRaftMembership, RaftMembershipError, RaftMembershipLimits};
use crate::log::measure_control_entry;
use sha2::{Digest, Sha256};
use wf_domain::workflow::control::{
    ControlPlan, ControlState, EntryIdentity, PreparedDigest, plan_control,
};
use wf_types::identity::{GroupId, RecoveryId};
use wf_types::version::LogIdentity;

/// Complete mathematical control changes borrowing the actual committed body.
pub struct GroupControlChanges<'a> {
    group: &'a GroupId,
    recovery: &'a RecoveryId,
    expected: ControlState,
    control: ControlPlan,
    encoded: &'a [u8],
    membership: bool,
}

impl<'a> GroupControlChanges<'a> {
    /// Validates the whole body, binds its SHA and plans the explicit stamp.
    /// Construction grants no committed-log provenance or durable authority.
    /// # Errors
    /// Refuses invalid input/control or a healthy explicit profile exhaustion.
    pub fn new(
        group: &'a GroupId,
        recovery: &'a RecoveryId,
        expected: ControlState,
        log: LogIdentity,
        body: &'a [u8],
        limits: RaftMembershipLimits,
    ) -> Result<Self, StateTransactionError> {
        let (stamp, membership) =
            measure_control_entry(body, group, recovery, limits).map_err(|error| match error {
                RaftMembershipError::Limit => StateTransactionError::Limit,
                RaftMembershipError::Allocation => StateTransactionError::Allocation,
                _ => StateTransactionError::InvalidChanges,
            })?;
        let identity = EntryIdentity::new(log, PreparedDigest::new(Sha256::digest(body).into()));
        let control = plan_control(expected, identity, stamp)
            .map_err(|_| StateTransactionError::InvalidChanges)?;
        Ok(Self {
            group,
            recovery,
            expected,
            control,
            encoded: body,
            membership: membership.is_some(),
        })
    }
    /// Fixed group whose authority the backend must compare.
    #[must_use]
    pub const fn group(&self) -> &'a GroupId {
        self.group
    }
    /// Fixed recovery generation whose authority the backend must compare.
    #[must_use]
    pub const fn recovery(&self) -> &'a RecoveryId {
        self.recovery
    }
    /// Exact expected same-guard predecessor.
    #[must_use]
    pub const fn expected(&self) -> ControlState {
        self.expected
    }
    /// Pure clock/checkpoint plan, including exact replay.
    #[must_use]
    pub const fn control(&self) -> ControlPlan {
        self.control
    }
    /// Borrows the complete original frame, never a reencoded body.
    #[must_use]
    pub const fn encoded(&self) -> &'a [u8] {
        self.encoded
    }
    /// Original FWMB body when this entry replaces last-applied membership.
    #[must_use]
    pub const fn membership_body(&self) -> Option<&'a [u8]> {
        if self.membership {
            Some(self.encoded)
        } else {
            None
        }
    }
}

/// Additional semantic capability on the SAME exclusive apply guard.
pub trait GroupControlTransaction: GroupApplyTransaction {
    /// Actual full configured cumulative R and C; available even when fenced.
    fn owner_limits(&self) -> (usize, usize);
    /// Reads complete last-applied membership under cumulative R/C and fencing.
    /// # Errors
    /// Refuses larger query profiles, exhaustion, corrupt data or unavailable authority.
    fn applied_membership(
        &self,
        limits: RaftMembershipLimits,
    ) -> Result<AppliedRaftMembership, StateTransactionError>;
    /// Atomically publishes membership and the SAME control, consuming this guard.
    /// # Errors
    /// Refuses incoherence, changed predecessor or unproved commit durability.
    fn commit_control(self, changes: &GroupControlChanges<'_>)
    -> Result<(), StateTransactionError>;
}
