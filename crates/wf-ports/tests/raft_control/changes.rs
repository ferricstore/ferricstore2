use super::{BLANK_SHA, TestResult, hex, packed, scope, stamp};
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp, PreparedDigest};
use wf_ports::consensus::RaftMembershipLimits;
use wf_ports::state::{GroupControlChanges, StateTransactionError};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

fn id(term: u64, node: u64, index: u64) -> LogIdentity {
    LogIdentity::new(term, node, AppliedPosition::new(index))
}

#[test]
fn control_changes_bind_actual_scope_bytes_and_original_stamp() -> TestResult {
    let (g, r) = scope()?;
    let limits = RaftMembershipLimits::default();
    let initial = ControlState::initial(CommittedTime::new(HlcTimestamp::new(17, 1)?, 16)?);
    let blank = packed(false, EntryStamp::Unstamped)?;
    let changes = GroupControlChanges::new(&g, &r, initial, id(1, 7, 0), &blank, limits)?;
    assert_eq!(changes.group(), &g);
    assert_eq!(changes.recovery(), &r);
    assert_eq!(changes.expected(), initial);
    assert_eq!(changes.encoded().as_ptr(), blank.as_ptr());
    assert!(changes.membership_body().is_none());
    let first = changes.control().next();
    assert_eq!(first.time(), initial.time());
    assert_eq!(
        first
            .last_entry()
            .ok_or("no first identity")?
            .prepared()
            .as_bytes()
            .as_slice(),
        hex(BLANK_SHA)?
    );
    assert_eq!(first.last_entry_stamp(), Some(EntryStamp::Unstamped));
    let member_stamp = stamp(20, 3, 19)?;
    let member = packed(true, member_stamp)?;
    let changes = GroupControlChanges::new(&g, &r, first, id(2, 0, 1), &member, limits)?;
    assert_eq!(
        changes.membership_body().ok_or("no member body")?.as_ptr(),
        member.as_ptr()
    );
    let second = changes.control().next();
    assert_eq!(
        second.time(),
        CommittedTime::new(HlcTimestamp::new(20, 3)?, 19)?
    );
    assert_eq!(second.last_entry_stamp(), Some(member_stamp));
    let replay = GroupControlChanges::new(&g, &r, second, id(2, 0, 1), &member, limits)?;
    assert!(replay.control().is_log_replay());
    assert_eq!(replay.control().next(), second);
    let low_stamp = stamp(18, 9, 17)?;
    let low = packed(false, low_stamp)?;
    let clamped = GroupControlChanges::new(&g, &r, second, id(2, 0, 2), &low, limits)?
        .control()
        .next();
    assert_eq!(clamped.time(), second.time());
    assert_eq!(clamped.last_entry_stamp(), Some(low_stamp));
    for (before, log, body) in [
        (initial, id(1, 7, 1), &blank),
        (second, id(1, 7, 2), &blank),
        (second, id(2, 0, 1), &low),
    ] {
        assert!(matches!(
            GroupControlChanges::new(&g, &r, before, log, body, limits),
            Err(StateTransactionError::InvalidChanges)
        ));
    }
    let foreign = wf_types::identity::GroupId::new(b"other".to_vec())?;
    assert!(matches!(
        GroupControlChanges::new(&foreign, &r, initial, id(1, 7, 0), &blank, limits),
        Err(StateTransactionError::InvalidChanges)
    ));
    let tiny = RaftMembershipLimits::new(3, 6, 4, 2, 1, 4661)?;
    assert!(matches!(
        GroupControlChanges::new(&g, &r, initial, id(1, 7, 0), &blank, tiny),
        Err(StateTransactionError::Limit)
    ));
    // A digest in an ordinary scalar constructor is not durable origin proof.
    let actual: [u8; 32] = hex(BLANK_SHA)?.try_into().map_err(|_| "digest width")?;
    assert_eq!(
        first.last_entry(),
        Some(EntryIdentity::new(id(1, 7, 0), PreparedDigest::new(actual)))
    );
    Ok(())
}
