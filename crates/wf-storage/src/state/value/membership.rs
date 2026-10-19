//! Required last-applied membership values retain the exact original FWMB body.

use super::{
    StateValueError as Error, add, allocate, finish, flag, read_u16, read_u64, start, take,
};
use sha2::{Digest, Sha256};
use wf_domain::workflow::control::{EntryIdentity, EntryStamp, PreparedDigest};
use wf_ports::consensus::{AppliedRaftMembership, RaftMembershipError, RaftMembershipLimits};
use wf_ports::log::{decode_raft_membership_body, measure_control_entry};
use wf_types::identity::{GroupId, RecoveryId};
use wf_types::version::{AppliedPosition, LogIdentity};

pub(crate) type MembershipParts<'a> = (EntryIdentity, EntryStamp, &'a [u8]);

pub(crate) fn membership_error(error: RaftMembershipError) -> Error {
    match error {
        RaftMembershipError::Limit => Error::MembershipLimit,
        RaftMembershipError::Allocation => Error::Allocation,
        RaftMembershipError::UnsupportedVersion => Error::UnsupportedVersion,
        RaftMembershipError::Malformed => Error::Malformed,
        _ => Error::InvalidValue,
    }
}

fn initial_owner(limits: RaftMembershipLimits) -> Result<usize, Error> {
    if limits.maximum_owned_bytes() < 1024 {
        return Err(Error::Limit {
            bytes: 1024,
            maximum: limits.maximum_owned_bytes(),
        });
    }
    Ok(1024)
}

/// Encodes initial metadata or the original complete membership body.
/// The maximum bounds this FWMV output; membership wire limits bound only FWMB.
/// # Errors
/// Refuses incoherent pairs, invalid body/profile, arithmetic or output admission.
pub fn encode_applied_membership(
    group: &GroupId,
    recovery: &RecoveryId,
    log_id: Option<LogIdentity>,
    body: Option<&[u8]>,
    limits: RaftMembershipLimits,
    maximum_bytes: usize,
) -> Result<Vec<u8>, Error> {
    let mut size = 43_usize;
    add(&mut size, group.as_bytes().len())?;
    add(&mut size, recovery.as_bytes().len())?;
    match (log_id, body) {
        (Some(_), Some(body)) => {
            if measure_control_entry(body, group, recovery, limits)
                .map_err(membership_error)?
                .1
                .is_none()
            {
                return Err(Error::InvalidValue);
            }
            u64::try_from(body.len()).map_err(|_| Error::LengthOverflow)?;
            add(&mut size, 32)?;
            add(&mut size, body.len())?;
        }
        (None, None) => {
            initial_owner(limits)?;
        }
        _ => return Err(Error::InvalidValue),
    }
    let mut output = allocate(size, maximum_bytes, *b"FWMV")?;
    for part in [group.as_bytes(), recovery.as_bytes()] {
        output.extend_from_slice(
            &u16::try_from(part.len())
                .map_err(|_| Error::LengthOverflow)?
                .to_be_bytes(),
        );
        output.extend_from_slice(part);
    }
    output.push(u8::from(log_id.is_some()));
    if let (Some(log), Some(body)) = (log_id, body) {
        output.extend_from_slice(&log.term().to_be_bytes());
        output.extend_from_slice(&log.leader_node().to_be_bytes());
        output.extend_from_slice(&log.index().get().to_be_bytes());
        output.extend_from_slice(
            &u64::try_from(body.len())
                .map_err(|_| Error::LengthOverflow)?
                .to_be_bytes(),
        );
        output.extend_from_slice(body);
    }
    let checksum = Sha256::digest(&output);
    output.extend_from_slice(&checksum);
    Ok(output)
}

pub(crate) fn inspect_membership<'a>(
    bytes: &'a [u8],
    group: &GroupId,
    recovery: &RecoveryId,
    limits: RaftMembershipLimits,
    owner: &mut usize,
) -> Result<Option<MembershipParts<'a>>, Error> {
    let mut at = start(bytes, *b"FWMV", bytes.len())?;
    for expected in [group.as_bytes(), recovery.as_bytes()] {
        let length = usize::from(read_u16(bytes, &mut at)?);
        if length == 0 || length > 256 || take(bytes, &mut at, length)? != expected {
            return Err(Error::InvalidValue);
        }
    }
    let result = if flag(bytes, &mut at)? {
        let term = read_u64(bytes, &mut at)?;
        let node = read_u64(bytes, &mut at)?;
        let index = read_u64(bytes, &mut at)?;
        let declared = read_u64(bytes, &mut at)?;
        let length = bytes
            .len()
            .checked_sub(at)
            .and_then(|n| n.checked_sub(32))
            .ok_or(Error::Malformed)?;
        if declared != u64::try_from(length).map_err(|_| Error::Malformed)? {
            return Err(Error::Malformed);
        }
        if length > limits.maximum_wire_bytes() {
            return Err(Error::Limit {
                bytes: length,
                maximum: limits.maximum_wire_bytes(),
            });
        }
        let body = take(bytes, &mut at, length)?;
        let (stamp, measured) =
            measure_control_entry(body, group, recovery, limits).map_err(membership_error)?;
        *owner = measured.ok_or(Error::InvalidValue)?;
        Some((
            LogIdentity::new(term, node, AppliedPosition::new(index)),
            stamp,
            body,
        ))
    } else {
        *owner = initial_owner(limits)?;
        None
    };
    let hash_at = at;
    let expected = take(bytes, &mut at, 32)?;
    finish(bytes, at)?;
    if expected != Sha256::digest(&bytes[..hash_at]).as_slice() {
        return Err(Error::InvalidValue);
    }
    Ok(result.map(|(log, stamp, body)| {
        (
            EntryIdentity::new(log, PreparedDigest::new(Sha256::digest(body).into())),
            stamp,
            body,
        )
    }))
}

/// Fully validates a borrowed value and measures its sole typed owner.
/// # Errors
/// Refuses framing, namespace, body/checksum correspondence or supported profile.
pub fn measure_applied_membership(
    bytes: &[u8],
    group: &GroupId,
    recovery: &RecoveryId,
    limits: RaftMembershipLimits,
) -> Result<usize, Error> {
    let mut owner = 0;
    inspect_membership(bytes, group, recovery, limits, &mut owner)?;
    Ok(owner)
}

/// Materializes exactly one complete typed owner after full validation/admission.
/// # Errors
/// Refuses invalid data, owner limits or an admitted allocation failure.
pub fn decode_applied_membership(
    bytes: &[u8],
    group: &GroupId,
    recovery: &RecoveryId,
    limits: RaftMembershipLimits,
    maximum_owned_bytes: usize,
) -> Result<AppliedRaftMembership, Error> {
    let mut owner = 0;
    let observed = inspect_membership(bytes, group, recovery, limits, &mut owner)?;
    if owner > maximum_owned_bytes {
        return Err(Error::Limit {
            bytes: owner,
            maximum: maximum_owned_bytes,
        });
    }
    let Some((entry, stamp, body)) = observed else {
        return Ok(AppliedRaftMembership::initial());
    };
    let mut workspace = 2560_usize;
    add(&mut workspace, body.len())?;
    add(&mut workspace, owner)?;
    let membership = decode_raft_membership_body(body, group, recovery, limits, workspace)
        .map_err(membership_error)?;
    Ok(AppliedRaftMembership::new(entry, stamp, membership))
}
