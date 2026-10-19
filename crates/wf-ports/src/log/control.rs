//! Closed scoped Blank/Membership bodies; ordinary FWCP bytes are unchanged.

use super::PreparedBytes;
use crate::consensus::{RaftMembership, RaftMembershipError, RaftMembershipLimits};
use wf_domain::workflow::control::EntryStamp;
use wf_types::identity::{GroupId, RecoveryId};

mod codec;
mod scan;

/// Validates and measures an actual body without an allocation or origin proof.
/// # Errors
/// Refuses framing, namespace, stamp, data correspondence or profile limits.
pub fn measure_control_entry(
    body: &[u8],
    group: &GroupId,
    recovery: &RecoveryId,
    limits: RaftMembershipLimits,
) -> Result<(EntryStamp, Option<usize>), RaftMembershipError> {
    scan::inspect(body, group, recovery, limits, &mut [0; 5])
}

/// Encodes an explicitly stamped or unstamped Blank, generating no clock values.
/// # Errors
/// Refuses data/workspace caps or output allocation failure.
pub fn encode_raft_blank(
    group: &GroupId,
    recovery: &RecoveryId,
    stamp: EntryStamp,
    limits: RaftMembershipLimits,
    workspace_bytes: usize,
) -> Result<PreparedBytes, RaftMembershipError> {
    codec::encode(group, recovery, stamp, None, limits, workspace_bytes)
}

/// Encodes complete node metadata and ordered configurations without a wrapper.
/// # Errors
/// Refuses malformed/empty membership, limits or allocation failure.
pub fn encode_raft_membership(
    group: &GroupId,
    recovery: &RecoveryId,
    stamp: EntryStamp,
    membership: &RaftMembership,
    limits: RaftMembershipLimits,
    workspace_bytes: usize,
) -> Result<PreparedBytes, RaftMembershipError> {
    codec::encode(
        group,
        recovery,
        stamp,
        Some(membership),
        limits,
        workspace_bytes,
    )
}

/// Materializes one admitted owner from a complete FWMB body.
/// # Errors
/// Refuses invalid bodies, workspace/profile limits or allocation failure.
pub fn decode_raft_membership_body(
    body: &[u8],
    group: &GroupId,
    recovery: &RecoveryId,
    limits: RaftMembershipLimits,
    workspace_bytes: usize,
) -> Result<RaftMembership, RaftMembershipError> {
    codec::decode(body, group, recovery, limits, workspace_bytes)
}
