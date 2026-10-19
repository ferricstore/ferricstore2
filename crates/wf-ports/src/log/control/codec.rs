//! Direct fallible emission and one admitted typed decode owner.

use super::scan;
use crate::consensus::{
    RaftMembership, RaftMembershipError as Error, RaftMembershipLimits, raft_membership_owned_bytes,
};
use crate::log::{PreparedBytes, digest};
use wf_domain::workflow::control::EntryStamp;
use wf_types::identity::{GroupId, RecoveryId};

fn reserve<T>(output: &mut Vec<T>, n: usize) -> Result<(), Error> {
    output.try_reserve_exact(n).map_err(|_| Error::Allocation)
}
fn count(n: usize) -> Result<[u8; 4], Error> {
    Ok(u32::try_from(n).map_err(|_| Error::Limit)?.to_be_bytes())
}
fn size(
    group: &GroupId,
    recovery: &RecoveryId,
    stamp: EntryStamp,
    member: Option<&RaftMembership>,
) -> Result<usize, Error> {
    let mut size = 11_usize
        .checked_add(group.as_bytes().len())
        .and_then(|n| n.checked_add(recovery.as_bytes().len()))
        .ok_or(Error::Limit)?;
    if matches!(stamp, EntryStamp::Stamped(_)) {
        size = size.checked_add(18).ok_or(Error::Limit)?;
    }
    if let Some(value) = member {
        count(value.configs().len())?;
        count(value.nodes().len())?;
        size = size.checked_add(8).ok_or(Error::Limit)?;
        for config in value.configs() {
            count(config.len())?;
            size = size
                .checked_add(4)
                .and_then(|n| n.checked_add(config.len().checked_mul(8)?))
                .ok_or(Error::Limit)?;
        }
        for (_, address) in value.nodes() {
            count(address.len())?;
            size = size
                .checked_add(12)
                .and_then(|n| n.checked_add(address.len()))
                .ok_or(Error::Limit)?;
        }
    }
    Ok(size)
}
pub(super) fn encode(
    group: &GroupId,
    recovery: &RecoveryId,
    stamp: EntryStamp,
    member: Option<&RaftMembership>,
    limits: RaftMembershipLimits,
    workspace: usize,
) -> Result<PreparedBytes, Error> {
    let owner = if let Some(value) = member {
        if value.configs().is_empty() {
            return Err(Error::Malformed);
        }
        raft_membership_owned_bytes(value, limits)?
    } else {
        0
    };
    let size = size(group, recovery, stamp, member)?;
    let peak = 2048_usize
        .checked_add(owner)
        .and_then(|n| n.checked_add(size))
        .ok_or(Error::Limit)?;
    if size > limits.maximum_wire_bytes() || peak > workspace {
        return Err(Error::Limit);
    }
    let mut bytes = Vec::new();
    reserve(&mut bytes, size)?;
    bytes.extend_from_slice(if member.is_some() { b"FWMB" } else { b"FWBL" });
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    for field in [group.as_bytes(), recovery.as_bytes()] {
        bytes.extend_from_slice(
            &u16::try_from(field.len())
                .map_err(|_| Error::Limit)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(field);
    }
    bytes.push(u8::from(matches!(stamp, EntryStamp::Stamped(_))));
    if let EntryStamp::Stamped(time) = stamp {
        bytes.extend_from_slice(&time.hlc().physical().get().to_be_bytes());
        bytes.extend_from_slice(&time.hlc().logical().to_be_bytes());
        bytes.extend_from_slice(&time.wall().get().to_be_bytes());
    }
    if let Some(value) = member {
        write(&mut bytes, value)?;
    }
    let hash = digest(&bytes);
    Ok(PreparedBytes {
        bytes,
        digest: hash,
    })
}
fn write(output: &mut Vec<u8>, value: &RaftMembership) -> Result<(), Error> {
    output.extend_from_slice(&count(value.configs().len())?);
    for config in value.configs() {
        output.extend_from_slice(&count(config.len())?);
        for id in config {
            output.extend_from_slice(&id.to_be_bytes());
        }
    }
    output.extend_from_slice(&count(value.nodes().len())?);
    for (id, _) in value.nodes() {
        output.extend_from_slice(&id.to_be_bytes());
    }
    for (_, address) in value.nodes() {
        output.extend_from_slice(&count(address.len())?);
        output.extend_from_slice(address.as_bytes());
    }
    Ok(())
}
pub(super) fn decode(
    bytes: &[u8],
    group: &GroupId,
    recovery: &RecoveryId,
    limits: RaftMembershipLimits,
    workspace: usize,
) -> Result<RaftMembership, Error> {
    let mut positions = [0; 5];
    let (_, owner) = scan::inspect(bytes, group, recovery, limits, &mut positions)?;
    let owner = owner.ok_or(Error::Malformed)?;
    let peak = [2048, 512, bytes.len(), owner]
        .into_iter()
        .try_fold(0_usize, usize::checked_add)
        .ok_or(Error::Limit)?;
    if peak > workspace {
        return Err(Error::Limit);
    }
    let [
        mut configs_at,
        mut ids_at,
        mut addresses_at,
        config_count,
        node_count,
    ] = positions;
    let mut nodes = Vec::new();
    reserve(&mut nodes, node_count)?;
    for _ in 0..node_count {
        let id = scan::u64(bytes, &mut ids_at)?;
        let address = scan::raw(bytes, &mut addresses_at)?;
        let mut text_bytes = Vec::new();
        reserve(&mut text_bytes, address.len())?;
        text_bytes.extend_from_slice(address);
        nodes.push((
            id,
            String::from_utf8(text_bytes).map_err(|_| Error::Malformed)?,
        ));
    }
    let mut configs = Vec::new();
    reserve(&mut configs, config_count)?;
    for _ in 0..config_count {
        let n = scan::u32(bytes, &mut configs_at)?;
        let mut config = Vec::new();
        reserve(&mut config, n)?;
        for _ in 0..n {
            config.push(scan::u64(bytes, &mut configs_at)?);
        }
        configs.push(config);
    }
    RaftMembership::new(configs, nodes, limits)
}
