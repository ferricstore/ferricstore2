//! ADR-004 deterministic control time and scoped complete log identity.
#![forbid(unsafe_code)]

use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::{
    ControlState, EntryIdentity, EntryStamp, PreparedDigest, plan_control,
};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn stamp(physical: u64, logical: u16, wall: u64) -> Result<EntryStamp, Box<dyn std::error::Error>> {
    Ok(EntryStamp::Stamped(CommittedTime::new(
        HlcTimestamp::new(physical, logical)?,
        wall,
    )?))
}

fn entry(term: u64, index: u64, byte: u8) -> EntryIdentity {
    EntryIdentity::new(
        LogIdentity::new(term, 7, AppliedPosition::new(index)),
        PreparedDigest::new([byte; 32]),
    )
}

#[test]
fn all_admissible_entry_classes_advance_lexical_hlc_and_independent_wall_max() -> TestResult {
    let initial = initialized(CommittedTime::new(HlcTimestamp::new(10, 4)?, 8)?)?;
    let first = plan_control(initial, entry(1, 1, 1), stamp(10, 3, 10)?)?;
    assert_eq!(first.next().time().hlc(), HlcTimestamp::new(10, 4)?);
    assert_eq!(first.next().time().wall().get(), 10);
    assert_eq!(first.execution_time().get(), 10);
    assert!(!first.is_log_replay());
    // Accepted/rejected/request-replay/tick/stamped-membership all use this same primitive.
    let mut state = first.next();
    for index in 2..=6 {
        let next = plan_control(
            state,
            entry(2, index, u8::try_from(index)?),
            stamp(11, 1, 7)?,
        )?;
        assert_eq!(next.next().time().hlc(), HlcTimestamp::new(11, 1)?);
        assert_eq!(next.next().time().wall().get(), 10);
        assert_eq!(
            next.execution_time().get(),
            11,
            "workflow execution follows committed HLC physical time"
        );
        assert_eq!(next.next().position().get(), index);
        assert_eq!(
            next,
            plan_control(
                state,
                entry(2, index, u8::try_from(index)?),
                stamp(11, 1, 7)?
            )?
        );
        state = next.next();
    }
    let unstamped = plan_control(state, entry(2, 7, 7), EntryStamp::Unstamped)?;
    assert_eq!(unstamped.next().time(), state.time());
    assert_eq!(unstamped.next().position().get(), 7);
    Ok(())
}

#[test]
fn exact_last_log_replay_preserves_all_control_and_conflicting_identity_rejects() -> TestResult {
    let initial = initialized(CommittedTime::new(HlcTimestamp::new(10, 0)?, 9)?)?;
    let committed = entry(8, 1, 1);
    let stamp = stamp(12, 1, 11)?;
    let applied = plan_control(initial, committed, stamp)?.next();
    let replay = plan_control(applied, committed, stamp)?;
    assert!(replay.is_log_replay());
    assert_eq!(replay.next(), applied);
    assert_eq!(
        plan_control(applied, entry(9, 1, 1), stamp),
        Err(ApplyError::PositionConflict)
    );
    let other_leader = EntryIdentity::new(
        LogIdentity::new(8, 9, AppliedPosition::new(1)),
        PreparedDigest::new([1; 32]),
    );
    assert_eq!(
        plan_control(applied, other_leader, stamp),
        Err(ApplyError::PositionConflict)
    );
    assert_eq!(
        plan_control(applied, entry(8, 1, 2), stamp),
        Err(ApplyError::PositionConflict)
    );
    assert_eq!(
        plan_control(applied, committed, EntryStamp::Unstamped),
        Err(ApplyError::PositionConflict)
    );
    assert_eq!(
        plan_control(applied, entry(8, 3, 3), stamp),
        Err(ApplyError::PositionConflict)
    );
    Ok(())
}

#[test]
fn generated_backwards_forward_and_logical_sequences_are_monotonic_and_replayable() -> TestResult {
    let origin = initialized(CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?)?;
    let mut left = origin;
    let mut right = origin;
    for index in 1_u64..=2_000 {
        let physical = (index * 73) % 997;
        let logical = u16::try_from((index * 19) % 65_536)?;
        let wall = physical / 2;
        let identity = entry(index / 100 + 1, index, u8::try_from(index % 256)?);
        let metadata = stamp(physical, logical, wall)?;
        let plan = plan_control(left, identity, metadata)?;
        let independent = plan_control(right, identity, metadata)?;
        assert_eq!(plan, independent);
        assert!(plan.next().time().hlc() >= left.time().hlc());
        assert!(plan.next().time().wall() >= left.time().wall());
        assert!(plan.next().time().wall() <= plan.next().time().hlc().physical());
        assert_eq!(plan.next().position().get(), index);
        left = plan.next();
        right = independent.next();
    }
    Ok(())
}

fn initialized(time: CommittedTime) -> Result<ControlState, Box<dyn std::error::Error>> {
    Ok(plan_control(
        ControlState::initial(time),
        EntryIdentity::new(
            LogIdentity::new(0, 0, AppliedPosition::new(0)),
            PreparedDigest::new([0; 32]),
        ),
        EntryStamp::Unstamped,
    )?
    .next())
}
