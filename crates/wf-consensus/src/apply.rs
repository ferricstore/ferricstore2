//! Same-guard bounded Blank/Membership application and applied-state observation.

use wf_domain::workflow::control::ControlState;
use wf_ports::consensus::{AppliedRaftMembership, RaftMembershipLimits};
use wf_ports::state::{GroupControlChanges, GroupControlTransaction, StateTransactionError};
use wf_types::identity::{GroupId, RecoveryId};
use wf_types::version::LogIdentity;

fn admit(
    read: usize,
    temporary: usize,
    entry: usize,
    workspace: usize,
) -> Result<(), StateTransactionError> {
    if workspace == 0 {
        return Err(StateTransactionError::InvalidLimits);
    }
    let needed = [read, temporary, entry]
        .into_iter()
        .try_fold(3072_usize, |total, value| {
            total.checked_add(value).ok_or(StateTransactionError::Limit)
        })?;
    if needed > workspace {
        return Err(StateTransactionError::Limit);
    }
    Ok(())
}

/// Applies one explicitly committed control body through the supplied real guard.
/// The caller establishes committed-log ordering; no local time is generated.
/// # Errors
/// Refuses workspace before reads/hash, input/control failures or uncertain commit.
pub fn apply_control_entry<T: GroupControlTransaction>(
    tx: T,
    group: &GroupId,
    recovery: &RecoveryId,
    body: &[u8],
    log: LogIdentity,
    limits: RaftMembershipLimits,
    workspace_bytes: usize,
) -> Result<ControlState, StateTransactionError> {
    let (read, temporary) = tx.owner_limits();
    let entry = body
        .len()
        .checked_add(512)
        .ok_or(StateTransactionError::Limit)?;
    admit(read, temporary, entry, workspace_bytes)?;
    let changes = GroupControlChanges::new(group, recovery, tx.control()?, log, body, limits)?;
    let next = changes.control().next();
    tx.commit_control(&changes)?;
    Ok(next)
}

/// Reads global control and its last-applied membership from the SAME guard.
/// # Errors
/// Refuses actual pool/workspace, supported-profile, corruption or backend failure.
pub fn read_applied_state<T: GroupControlTransaction>(
    tx: &T,
    limits: RaftMembershipLimits,
    workspace_bytes: usize,
) -> Result<(ControlState, AppliedRaftMembership), StateTransactionError> {
    let (read, temporary) = tx.owner_limits();
    admit(read, temporary, 0, workspace_bytes)?;
    Ok((tx.control()?, tx.applied_membership(limits)?))
}
