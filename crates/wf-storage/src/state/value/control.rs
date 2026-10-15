//! Group-scoped control values, including the original unclamped entry stamp.

use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp, PreparedDigest};
use wf_types::identity::{GroupId, RecoveryId};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

use super::{
    StateValueError, add, allocate, finish, flag, put_raw, raw, raw_size, read_u16, read_u64,
    start, take,
};

/// Encodes a complete group/recovery control value using the FWSC1 grammar.
///
/// # Errors
/// Rejects a zero/tight byte cap, arithmetic overflow or output allocation failure.
pub fn encode_control(
    group: &GroupId,
    recovery: &RecoveryId,
    control: ControlState,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    if maximum_bytes == 0 {
        return Err(StateValueError::InvalidValue);
    }
    let last = match (control.last_entry(), control.last_entry_stamp()) {
        (Some(identity), Some(stamp)) => Some((identity, stamp)),
        (None, None) => None,
        _ => return Err(StateValueError::InvalidValue),
    };
    let mut size = 6 + 18 + 1;
    add(&mut size, raw_size(group.as_bytes())?)?;
    add(&mut size, raw_size(recovery.as_bytes())?)?;
    if let Some((_, stamp)) = last {
        add(&mut size, 24 + 32 + 1)?;
        if matches!(stamp, EntryStamp::Stamped(_)) {
            add(&mut size, 18)?;
        }
    }
    let mut output = allocate(size, maximum_bytes, *b"FWSC")?;
    put_raw(&mut output, group.as_bytes())?;
    put_raw(&mut output, recovery.as_bytes())?;
    put_time(&mut output, control.time());
    output.push(u8::from(last.is_some()));
    if let Some((identity, stamp)) = last {
        let log = identity.log();
        output.extend_from_slice(&log.term().to_be_bytes());
        output.extend_from_slice(&log.leader_node().to_be_bytes());
        output.extend_from_slice(&log.index().get().to_be_bytes());
        output.extend_from_slice(identity.prepared().as_bytes());
        output.push(u8::from(matches!(stamp, EntryStamp::Stamped(_))));
        if let EntryStamp::Stamped(time) = stamp {
            put_time(&mut output, time);
        }
    }
    Ok(output)
}

/// Restores control only after complete framing and expected namespace checks.
///
/// # Errors
/// Rejects unsupported/malformed values, foreign scope, invalid clocks and limits.
pub fn decode_control(
    bytes: &[u8],
    expected_group: &GroupId,
    expected_recovery: &RecoveryId,
    maximum_bytes: usize,
) -> Result<ControlState, StateValueError> {
    let mut offset = start(bytes, *b"FWSC", maximum_bytes)?;
    let group = raw(bytes, &mut offset)?;
    let recovery = raw(bytes, &mut offset)?;
    let current = read_time(bytes, &mut offset)?;
    let last = if flag(bytes, &mut offset)? {
        let term = read_u64(bytes, &mut offset)?;
        let node = read_u64(bytes, &mut offset)?;
        let index = read_u64(bytes, &mut offset)?;
        let digest: [u8; 32] = take(bytes, &mut offset, 32)?
            .try_into()
            .map_err(|_| StateValueError::Malformed)?;
        let stamp = if flag(bytes, &mut offset)? {
            Some(read_time(bytes, &mut offset)?)
        } else {
            None
        };
        Some((term, node, index, digest, stamp))
    } else {
        None
    };
    finish(bytes, offset)?;
    if group != expected_group.as_bytes() || recovery != expected_recovery.as_bytes() {
        return Err(StateValueError::InvalidValue);
    }
    let current = time(current)?;
    let Some((term, node, index, digest, stamp)) = last else {
        return Ok(ControlState::initial(current));
    };
    let stamp = stamp
        .map(time)
        .transpose()?
        .map_or(EntryStamp::Unstamped, EntryStamp::Stamped);
    ControlState::restored(
        current,
        EntryIdentity::new(
            LogIdentity::new(term, node, AppliedPosition::new(index)),
            PreparedDigest::new(digest),
        ),
        stamp,
    )
    .map_err(|_| StateValueError::InvalidValue)
}

fn put_time(output: &mut Vec<u8>, time: CommittedTime) {
    output.extend_from_slice(&time.hlc().physical().get().to_be_bytes());
    output.extend_from_slice(&time.hlc().logical().to_be_bytes());
    output.extend_from_slice(&time.wall().get().to_be_bytes());
}

fn read_time(bytes: &[u8], offset: &mut usize) -> Result<(u64, u16, u64), StateValueError> {
    Ok((
        read_u64(bytes, offset)?,
        read_u16(bytes, offset)?,
        read_u64(bytes, offset)?,
    ))
}

fn time((physical, logical, wall): (u64, u16, u64)) -> Result<CommittedTime, StateValueError> {
    let hlc = HlcTimestamp::new(physical, logical).map_err(|_| StateValueError::InvalidValue)?;
    CommittedTime::new(hlc, wall).map_err(|_| StateValueError::InvalidValue)
}
