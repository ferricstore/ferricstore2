//! Owned mathematical observation, never a commitment or purge token.

use super::SnapshotError;
use crate::consensus::AppliedRaftMembership;
use wf_domain::workflow::control::{ControlState, EntryStamp};
use wf_types::identity::{GroupId, RecoveryId};

/// Complete immutable snapshot observation with one transferred membership owner.
pub struct SnapshotMetadata {
    group: GroupId,
    recovery: RecoveryId,
    control: ControlState,
    membership: AppliedRaftMembership,
    id: [u8; 32],
    counts: (usize, usize),
}

impl SnapshotMetadata {
    /// Moves existing owners and checks control/membership consistency.
    /// This does not prove storage, commitment, source lineage or authorization.
    /// # Errors
    /// Refuses empty counts, membership ahead of control, or incoherent stamps.
    pub fn new(
        scope: (GroupId, RecoveryId),
        control: ControlState,
        membership: AppliedRaftMembership,
        id: [u8; 32],
        counts: (usize, usize),
    ) -> Result<Self, SnapshotError> {
        if counts.0 == 0 || counts.1 == 0 {
            return Err(SnapshotError::Malformed);
        }
        match (membership.entry(), membership.stamp()) {
            (None, None) => {
                if !membership.membership().configs().is_empty()
                    || !membership.membership().nodes().is_empty()
                {
                    return Err(SnapshotError::Malformed);
                }
            }
            (Some(entry), Some(stamp)) => {
                let applied = control.last_entry().ok_or(SnapshotError::Malformed)?;
                if membership.membership().configs().is_empty()
                    || entry.log().index() > applied.log().index()
                    || (entry.log().index() == applied.log().index()
                        && (entry != applied || control.last_entry_stamp() != Some(stamp)))
                {
                    return Err(SnapshotError::Malformed);
                }
                if let EntryStamp::Stamped(time) = stamp
                    && (control.time().hlc() < time.hlc() || control.time().wall() < time.wall())
                {
                    return Err(SnapshotError::Malformed);
                }
            }
            _ => return Err(SnapshotError::Malformed),
        }
        Ok(Self {
            group: scope.0,
            recovery: scope.1,
            control,
            membership,
            id,
            counts,
        })
    }
    /// Fixed group identity.
    #[must_use]
    pub const fn group(&self) -> &GroupId {
        &self.group
    }
    /// Fixed recovery identity.
    #[must_use]
    pub const fn recovery(&self) -> &RecoveryId {
        &self.recovery
    }
    /// Complete captured applied control.
    #[must_use]
    pub const fn control(&self) -> ControlState {
        self.control
    }
    /// Complete original last-applied membership.
    #[must_use]
    pub const fn membership(&self) -> &AppliedRaftMembership {
        &self.membership
    }
    /// SHA256 of the entire canonical stream, including its footer.
    #[must_use]
    pub const fn id(&self) -> &[u8; 32] {
        &self.id
    }
    /// Artifact rows, including both metadata rows.
    #[must_use]
    pub const fn row_count(&self) -> usize {
        self.counts.0
    }
    /// Complete canonical stream length, including framing and footer.
    #[must_use]
    pub const fn encoded_bytes(&self) -> usize {
        self.counts.1
    }
    /// Transfers the existing membership owner without copying it.
    #[must_use]
    pub fn into_membership(self) -> AppliedRaftMembership {
        self.membership
    }
}
