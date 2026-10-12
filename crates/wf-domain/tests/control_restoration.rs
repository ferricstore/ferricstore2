//! Real zero genesis, restored complete identity/time and global authority checks.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::{
    ControlState, EntryIdentity, EntryStamp, PreparedDigest, plan_control,
};
use wf_domain::workflow::request::gate::{CommittedHeader, GateView, inspect_request};
use wf_domain::workflow::request::{
    ClientIntent, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity, TimestampMillis,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn time(physical: u64, wall: u64) -> Result<CommittedTime, Box<dyn std::error::Error>> {
    Ok(CommittedTime::new(HlcTimestamp::new(physical, 0)?, wall)?)
}
fn identity(index: u64) -> EntryIdentity {
    EntryIdentity::new(
        LogIdentity::new(0, 0, AppliedPosition::new(index)),
        PreparedDigest::new([0; 32]),
    )
}

#[test]
fn genuinely_unapplied_control_admits_real_zero_genesis_and_replays_it() -> TestResult {
    let initial = ControlState::initial(time(0, 0)?);
    assert_eq!(initial.last_entry(), None);
    let genesis = plan_control(initial, identity(0), EntryStamp::Unstamped)?;
    assert!(!genesis.is_log_replay());
    assert_eq!(genesis.next().position().get(), 0);
    assert_eq!(genesis.next().last_entry(), Some(identity(0)));
    let replay = plan_control(genesis.next(), identity(0), EntryStamp::Unstamped)?;
    assert!(replay.is_log_replay());
    assert_eq!(replay.next(), genesis.next());
    assert_eq!(
        plan_control(
            genesis.next(),
            identity(1),
            EntryStamp::Stamped(time(10, 8)?)
        )?
        .next()
        .position()
        .get(),
        1
    );
    Ok(())
}

#[test]
fn index_one_is_a_gap_while_genuinely_unapplied() -> TestResult {
    assert_eq!(
        plan_control(
            ControlState::initial(time(0, 0)?),
            identity(1),
            EntryStamp::Unstamped
        ),
        Err(ApplyError::PositionConflict)
    );
    Ok(())
}

#[test]
fn restored_control_preserves_full_replay_time_and_resume_without_overflow() -> TestResult {
    let last_stamp = EntryStamp::Stamped(time(10, 10)?);
    let restored = ControlState::restored(time(20, 18)?, identity(7), last_stamp)?;
    assert_eq!(restored.position().get(), 7);
    assert_eq!(restored.last_entry(), Some(identity(7)));
    let replay = plan_control(restored, identity(7), last_stamp)?;
    assert!(replay.is_log_replay());
    assert_eq!(replay.next(), restored);
    let resumed = plan_control(restored, identity(8), EntryStamp::Stamped(time(15, 15)?))?;
    assert_eq!(resumed.execution_time().get(), 20);
    assert_eq!(resumed.next().time().wall().get(), 18);
    assert_eq!(
        ControlState::restored(time(9, 8)?, identity(7), last_stamp),
        Err(ApplyError::InvalidAuthority)
    );
    assert_eq!(
        ControlState::restored(time(11, 9)?, identity(7), last_stamp),
        Err(ApplyError::InvalidAuthority)
    );
    let end = ControlState::restored(time(20, 18)?, identity(u64::MAX), EntryStamp::Unstamped)?;
    assert_eq!(
        plan_control(end, identity(0), EntryStamp::Unstamped),
        Err(ApplyError::PositionConflict)
    );
    Ok(())
}

#[test]
fn unapplied_scope_cannot_hide_retained_authority_under_an_unqueried_request_key() -> TestResult {
    let scope = RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    );
    let old_request = RequestId::new(b"old".to_vec())?;
    let queried = RequestId::new(b"missing".to_vec())?;
    let intent = ClientIntent::new(1, [1; 32])?;
    let retained = RetainedOutcome::new(
        RetainedMetadata {
            scope: scope.clone(),
            request: old_request.clone(),
            intent,
            original_position: identity(0).log(),
            original_time: TimestampMillis::new(5),
            expires_at: TimestampMillis::new(100),
            status: RetainedStatus::Accepted,
        },
        1,
        b"old-bytes".to_vec(),
        AdmissionLimits::default(),
    )?;
    let outcomes = BTreeMap::from([(old_request, retained)]);
    let view = GateView {
        scope: &scope,
        control: ControlState::initial(time(10, 10)?),
        outcomes: &outcomes,
    };
    let header = CommittedHeader {
        format_version: 1,
        scope: &scope,
        request: &queried,
        intent,
        identity: identity(0),
        stamp: time(10, 10)?,
        encoded_entry: b"entry",
        encoded_isolation_scope: b"scope",
    };
    assert_eq!(
        inspect_request(view, header, AdmissionLimits::default()),
        Err(ApplyError::InvalidAuthority)
    );
    Ok(())
}
