//! Independent closed control-entry vectors and mathematical checkpoint changes.

#![forbid(unsafe_code)]

use wf_domain::workflow::control::EntryStamp;
use wf_ports::consensus::{RaftMembership, RaftMembershipLimits};
use wf_types::identity::{GroupId, RecoveryId};
use wf_types::version::{CommittedTime, HlcTimestamp};

mod changes;
mod codec;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const MEMBER: &str = "46574d42000100016700017a000000000300000002000000000000000100000000000000020000000200000000000000020000000000000003000000020000000000000001000000000000000200000004000000000000000100000000000000020000000000000003000000000000000400000001610000000000000002780000000002ceb2";
const MEMBER_SHA: &str = "ec550282ecf2cb11f71e4f9a9aa2dac6318ab8772c0da4c432d9c30175a3edbe";
const BLANK: &str = "4657424c000100016700017a00";
const BLANK_SHA: &str = "e96cab9ab205c4d7b2e2bbd71f7f7a2c7586f84a88db0b2c0fed301192389d0c";

fn scope() -> TestResult<(GroupId, RecoveryId)> {
    Ok((
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    ))
}

fn hex(s: &str) -> TestResult<Vec<u8>> {
    s.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| Ok(u8::from_str_radix(std::str::from_utf8(p)?, 16)?))
        .collect()
}

fn stamp(physical: u64, logical: u16, wall: u64) -> TestResult<EntryStamp> {
    Ok(EntryStamp::Stamped(CommittedTime::new(
        HlcTimestamp::new(physical, logical)?,
        wall,
    )?))
}

fn nodes() -> Vec<(u64, String)> {
    vec![
        (1, "a".into()),
        (2, String::new()),
        (3, "x\0".into()),
        (4, "β".into()),
    ]
}

fn membership() -> TestResult<RaftMembership> {
    Ok(RaftMembership::new(
        vec![vec![1, 2], vec![2, 3], vec![1, 2]],
        nodes(),
        RaftMembershipLimits::default(),
    )?)
}

fn packed(member: bool, stamp: EntryStamp) -> TestResult<Vec<u8>> {
    let mut body = hex(if member { MEMBER } else { BLANK })?;
    if let EntryStamp::Stamped(time) = stamp {
        body[12] = 1;
        let mut time_wire = Vec::new();
        time_wire.extend_from_slice(&time.hlc().physical().get().to_be_bytes());
        time_wire.extend_from_slice(&time.hlc().logical().to_be_bytes());
        time_wire.extend_from_slice(&time.wall().get().to_be_bytes());
        body.splice(13..13, time_wire);
    }
    Ok(body)
}
