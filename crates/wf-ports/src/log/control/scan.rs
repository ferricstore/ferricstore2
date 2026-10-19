//! Fixed-width node lookup permits complete borrowed validation without an index.

use crate::consensus::{RaftMembershipError as Error, RaftMembershipLimits};
use wf_domain::workflow::control::EntryStamp;
use wf_types::identity::{GroupId, RecoveryId};
use wf_types::version::{CommittedTime, HlcTimestamp};

pub(super) fn take<'a>(bytes: &'a [u8], at: &mut usize, n: usize) -> Result<&'a [u8], Error> {
    let end = at.checked_add(n).ok_or(Error::Limit)?;
    let part = bytes.get(*at..end).ok_or(Error::Malformed)?;
    *at = end;
    Ok(part)
}
pub(super) fn u32(bytes: &[u8], at: &mut usize) -> Result<usize, Error> {
    usize::try_from(u32::from_be_bytes(
        take(bytes, at, 4)?
            .try_into()
            .map_err(|_| Error::Malformed)?,
    ))
    .map_err(|_| Error::Limit)
}
pub(super) fn u64(bytes: &[u8], at: &mut usize) -> Result<u64, Error> {
    Ok(u64::from_be_bytes(
        take(bytes, at, 8)?
            .try_into()
            .map_err(|_| Error::Malformed)?,
    ))
}
pub(super) fn raw<'a>(bytes: &'a [u8], at: &mut usize) -> Result<&'a [u8], Error> {
    let n = u32(bytes, at)?;
    take(bytes, at, n)
}

fn header(
    bytes: &[u8],
    at: &mut usize,
    group: &GroupId,
    recovery: &RecoveryId,
) -> Result<(EntryStamp, bool), Error> {
    let member = match take(bytes, at, 4)? {
        b"FWBL" => false,
        b"FWMB" => true,
        _ => return Err(Error::Malformed),
    };
    if take(bytes, at, 2)? != [0, 1] {
        return Err(Error::UnsupportedVersion);
    }
    for expected in [group.as_bytes(), recovery.as_bytes()] {
        let n = usize::from(u16::from_be_bytes(
            take(bytes, at, 2)?
                .try_into()
                .map_err(|_| Error::Malformed)?,
        ));
        if n == 0 || n > 256 {
            return Err(Error::Malformed);
        }
        if take(bytes, at, n)? != expected {
            return Err(Error::NamespaceMismatch);
        }
    }
    let stamp = match take(bytes, at, 1)? {
        [0] => EntryStamp::Unstamped,
        [1] => {
            let physical = u64(bytes, at)?;
            let logical = u16::from_be_bytes(
                take(bytes, at, 2)?
                    .try_into()
                    .map_err(|_| Error::Malformed)?,
            );
            let wall = u64(bytes, at)?;
            EntryStamp::Stamped(
                CommittedTime::new(
                    HlcTimestamp::new(physical, logical).map_err(|_| Error::Malformed)?,
                    wall,
                )
                .map_err(|_| Error::Malformed)?,
            )
        }
        _ => return Err(Error::Malformed),
    };
    Ok((stamp, member))
}

pub(super) fn inspect(
    bytes: &[u8],
    group: &GroupId,
    recovery: &RecoveryId,
    limits: RaftMembershipLimits,
    positions: &mut [usize; 5],
) -> Result<(EntryStamp, Option<usize>), Error> {
    if bytes.len() > limits.maximum_wire_bytes() {
        return Err(Error::Limit);
    }
    let mut at = 0;
    let (stamp, member) = header(bytes, &mut at, group, recovery)?;
    let owner = if member {
        Some(membership(bytes, &mut at, limits, positions)?)
    } else {
        None
    };
    if at != bytes.len() {
        return Err(Error::Malformed);
    }
    Ok((stamp, owner))
}

fn membership(
    bytes: &[u8],
    at: &mut usize,
    limits: RaftMembershipLimits,
    positions: &mut [usize; 5],
) -> Result<usize, Error> {
    let configs = u32(bytes, at)?;
    if configs == 0 {
        return Err(Error::Malformed);
    }
    if configs > limits.maximum_configs() {
        return Err(Error::Limit);
    }
    let config_start = *at;
    let mut voters = 0_usize;
    for _ in 0..configs {
        let count = u32(bytes, at)?;
        if count == 0 {
            return Err(Error::Malformed);
        }
        voters = voters.checked_add(count).ok_or(Error::Limit)?;
        if voters > limits.maximum_voter_occurrences() {
            return Err(Error::Limit);
        }
        let mut previous = None;
        for _ in 0..count {
            let id = u64(bytes, at)?;
            if previous.is_some_and(|before| before >= id) {
                return Err(Error::Malformed);
            }
            previous = Some(id);
        }
    }
    let nodes = u32(bytes, at)?;
    if nodes > limits.maximum_nodes() {
        return Err(Error::Limit);
    }
    let node_start = *at;
    let ids = take(bytes, at, nodes.checked_mul(8).ok_or(Error::Limit)?)?;
    let mut previous = None;
    for index in 0..nodes {
        let id = id_at(ids, index)?;
        if previous.is_some_and(|before| before >= id) {
            return Err(Error::Malformed);
        }
        previous = Some(id);
    }
    let address_start = *at;
    let mut addresses = 0_usize;
    for _ in 0..nodes {
        let value = raw(bytes, at)?;
        if value.len() > limits.maximum_address_bytes() {
            return Err(Error::Limit);
        }
        std::str::from_utf8(value).map_err(|_| Error::Malformed)?;
        addresses = addresses.checked_add(value.len()).ok_or(Error::Limit)?;
    }
    let mut cursor = config_start;
    for _ in 0..configs {
        let count = u32(bytes, &mut cursor)?;
        for _ in 0..count {
            if !contains(ids, nodes, u64(bytes, &mut cursor)?)? {
                return Err(Error::Malformed);
            }
        }
    }
    *positions = [config_start, node_start, address_start, configs, nodes];
    let owned = [
        1024,
        configs.checked_mul(512).ok_or(Error::Limit)?,
        voters.checked_mul(8).ok_or(Error::Limit)?,
        nodes.checked_mul(512).ok_or(Error::Limit)?,
        addresses,
    ]
    .into_iter()
    .try_fold(0_usize, usize::checked_add)
    .ok_or(Error::Limit)?;
    if owned > limits.maximum_owned_bytes() {
        return Err(Error::Limit);
    }
    Ok(owned)
}
fn id_at(ids: &[u8], index: usize) -> Result<u64, Error> {
    u64(ids, &mut index.checked_mul(8).ok_or(Error::Limit)?)
}
fn contains(ids: &[u8], count: usize, target: u64) -> Result<bool, Error> {
    let (mut low, mut high) = (0, count);
    while low < high {
        let mid = low + (high - low) / 2;
        match id_at(ids, mid)?.cmp(&target) {
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid,
            std::cmp::Ordering::Equal => return Ok(true),
        }
    }
    Ok(false)
}
