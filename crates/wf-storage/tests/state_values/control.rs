//! Independent FWSC vectors preserve group scope and the original entry stamp.

use wf_domain::workflow::control::{
    ControlState, EntryIdentity, EntryStamp, PreparedDigest, plan_control,
};
use wf_storage::state::value::{StateValueError, decode_control, encode_control};
use wf_types::identity::{GroupId, RecoveryId};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

use super::TestResult;

const INITIAL: &str = "46575343000100000001670000000172000000000000000a0002000000000000000700";
const UNSTAMPED: &str = "46575343000100000001670000000172000000000000000a0002000000000000000701000000000000000300000000000000040000000000000000aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa00";
const STAMPED: &str = "46575343000100000001670000000172000000000000000a0002000000000000000701000000000000000300000000000000040000000000000000aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01000000000000000900010000000000000006";

fn hex(value: &str) -> TestResult<Vec<u8>> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn clock(physical: u64, logical: u16, wall: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(
        HlcTimestamp::new(physical, logical)?,
        wall,
    )?)
}

fn identity() -> EntryIdentity {
    EntryIdentity::new(
        LogIdentity::new(3, 4, AppliedPosition::new(0)),
        PreparedDigest::new([0xaa; 32]),
    )
}

fn state(stamp: Option<EntryStamp>) -> TestResult<ControlState> {
    let current = clock(10, 2, 7)?;
    Ok(match stamp {
        None => ControlState::initial(current),
        Some(stamp) => ControlState::restored(current, identity(), stamp)?,
    })
}

#[test]
fn control_vectors_preserve_group_and_exact_last_stamp() -> TestResult {
    let group = GroupId::new(b"g".to_vec())?;
    let recovery = RecoveryId::new(b"r".to_vec())?;
    let backward = EntryStamp::Stamped(clock(9, 1, 6)?);
    // Independently packed before the selected codec/getter bodies exist.
    for (stamp, literal, size) in [
        (None, INITIAL, 35),
        (Some(EntryStamp::Unstamped), UNSTAMPED, 92),
        (Some(backward), STAMPED, 110),
    ] {
        let expected = state(stamp)?;
        let bytes = encode_control(&group, &recovery, expected, 1_024)?;
        assert_eq!(bytes, hex(literal)?);
        assert_eq!(bytes.len(), size);
        let decoded: ControlState = decode_control(&bytes, &group, &recovery, size)?;
        drop(bytes);
        assert_eq!(decoded, expected);
        assert_eq!(decoded.last_entry_stamp(), stamp);
        assert_eq!(decoded.last_entry().is_some(), stamp.is_some());
        assert_eq!(decoded.position().get(), 0);
        if let Some(stamp) = stamp {
            assert!(plan_control(decoded, identity(), stamp)?.is_log_replay());
        }
    }
    let decoded = decode_control(&hex(STAMPED)?, &group, &recovery, 110)?;
    assert!(plan_control(decoded, identity(), EntryStamp::Stamped(clock(10, 2, 7)?)).is_err());
    let max_group = GroupId::new(vec![0xff; 256])?;
    let max_recovery = RecoveryId::new(vec![0; 256])?;
    let expected = state(Some(backward))?;
    let maximum = encode_control(&max_group, &max_recovery, expected, 1_024)?;
    assert_eq!(maximum.len(), 620);
    assert_eq!(
        decode_control(&maximum, &max_group, &max_recovery, 620)?,
        expected
    );
    let binary_group = GroupId::new(vec![0, 0xff, b':'])?;
    let binary_recovery = RecoveryId::new(vec![0xff, b':', 0])?;
    let binary = encode_control(&binary_group, &binary_recovery, expected, 1_024)?;
    assert_eq!(
        decode_control(&binary, &binary_group, &binary_recovery, 1_024)?,
        expected
    );
    Ok(())
}

#[test]
fn control_corruption_and_byte_limits_fail_before_owners() -> TestResult {
    let group = GroupId::new(b"g".to_vec())?;
    let recovery = RecoveryId::new(b"r".to_vec())?;
    let bytes = hex(STAMPED)?;
    for prefix in 0..bytes.len() {
        assert!(decode_control(&bytes[..prefix], &group, &recovery, 1_024).is_err());
    }
    for (offset, replacement) in [(0, b'X'), (34, 2), (91, 2)] {
        let mut changed = bytes.clone();
        changed[offset] = replacement;
        assert!(decode_control(&changed, &group, &recovery, 1_024).is_err());
    }
    let mut version = bytes.clone();
    version[5] = 2;
    assert!(matches!(
        decode_control(&version, &group, &recovery, 1_024),
        Err(StateValueError::UnsupportedVersion)
    ));
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        decode_control(&trailing, &group, &recovery, 1_024),
        Err(StateValueError::Malformed)
    ));
    let foreign = GroupId::new(b"other".to_vec())?;
    let foreign_recovery = RecoveryId::new(b"other".to_vec())?;
    assert!(matches!(
        decode_control(&bytes, &foreign, &recovery, 1_024),
        Err(StateValueError::InvalidValue)
    ));
    assert!(matches!(
        decode_control(&bytes, &group, &foreign_recovery, 1_024),
        Err(StateValueError::InvalidValue)
    ));
    let mut old_clock = bytes.clone();
    old_clock[23] = 8;
    assert!(matches!(
        decode_control(&old_clock, &group, &recovery, 1_024),
        Err(StateValueError::InvalidValue)
    ));
    let expected = state(Some(EntryStamp::Stamped(clock(9, 1, 6)?)))?;
    assert!(matches!(
        encode_control(&group, &recovery, expected, 109),
        Err(StateValueError::Limit {
            bytes: 110,
            maximum: 109
        })
    ));
    assert!(matches!(
        decode_control(&bytes, &group, &recovery, 109),
        Err(StateValueError::Limit {
            bytes: 110,
            maximum: 109
        })
    ));
    assert_eq!(encode_control(&group, &recovery, expected, 110)?, bytes);
    assert!(matches!(
        encode_control(&group, &recovery, expected, 0),
        Err(StateValueError::InvalidValue)
    ));
    assert!(matches!(
        decode_control(&bytes, &group, &recovery, 0),
        Err(StateValueError::InvalidValue)
    ));
    Ok(())
}
