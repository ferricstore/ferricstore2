use super::{TestResult, fixtures};
use wf_consensus::apply::{apply_control_entry, read_applied_state};
use wf_consensus::recovery::{CheckpointJoinError, CheckpointJoinLimits, verify_checkpoint};
use wf_domain::workflow::{
    admission::AdmissionLimits,
    control::{ControlState, EntryIdentity, EntryStamp},
};
use wf_ports::consensus::{RaftMembership, RaftMembershipLimits};
use wf_ports::log::{PreparedBytes, encode_raft_membership};
use wf_ports::log::{RaftLogEntry, RaftLogStore};
use wf_ports::state::StateTransactionError;
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, GroupControlTransaction};
use wf_storage::state::RedbStateStore;

#[test]
fn checkpoint_join_admits_before_copy() -> TestResult {
    for args in [
        (0, 13, 627, 4215),
        (1, 0, 627, 4215),
        (1, 13, 511, 4215),
        (1, 13, 627, 0),
    ] {
        assert_eq!(
            CheckpointJoinLimits::new(args.0, args.1, args.2, args.3),
            Err(CheckpointJoinError::InvalidLimits)
        );
    }
    let default = CheckpointJoinLimits::default();
    assert_eq!(
        (
            default.maximum_read_steps(),
            default.maximum_temporary_bytes(),
            default.maximum_owned_entry_bytes(),
            default.workspace_bytes()
        ),
        (1, 2048, 16_777_728, 268_435_456)
    );
    let dir = fixtures::directory("caps")?;
    let (mut log, mut state) = fixtures::create_pair(&dir)?;
    let frame = fixtures::frame(b"i", b"q")?;
    log.append(vec![RaftLogEntry::new(
        fixtures::id(1, 7, 0),
        frame.as_bytes().to_vec(),
    )])?;
    log.save_committed(fixtures::id(1, 7, 0))?;
    let expected = fixtures::apply(&mut state, &frame, fixtures::id(1, 7, 0))?;
    let meta = log.state()?;
    drop((log, state));
    for (c, j, w, error) in [
        (13, 627, 4215, None),
        (13, 627, 4214, Some(CheckpointJoinError::Limit)),
        (
            13,
            626,
            4215,
            Some(CheckpointJoinError::Log(wf_ports::log::RaftLogError::Limit)),
        ),
        (
            12,
            627,
            4215,
            Some(CheckpointJoinError::State(StateTransactionError::Limit)),
        ),
        (2048, 627, 5636, None),
        (2048, 627, 5635, Some(CheckpointJoinError::Limit)),
        (13, 627, usize::MAX, None),
        (
            usize::MAX,
            627,
            usize::MAX,
            Some(CheckpointJoinError::Limit),
        ),
    ] {
        let (log, state) = fixtures::open_pair(&dir)?;
        let s = fixtures::scope()?;
        let result = verify_checkpoint(
            log,
            state,
            s.group(),
            s.recovery(),
            CheckpointJoinLimits::new(1, c, j, w)?,
        );
        match (result, error) {
            (Ok(pair), None) => drop(pair),
            (Err(actual), Some(expected)) => assert_eq!(actual, expected),
            _ => return Err(format!("wrong cap result: C{c}/J{j}/W{w}").into()),
        }
        let (mut log, mut state) = fixtures::open_pair(&dir)?;
        fixtures::assert_log(&mut log, meta, &[&frame])?;
        fixtures::assert_authority(&mut state, expected, true)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn membership_frame() -> TestResult<PreparedBytes> {
    let s = fixtures::scope()?;
    let limits = RaftMembershipLimits::default();
    let value = RaftMembership::new(
        vec![vec![1, 2], vec![2, 3], vec![1, 2]],
        vec![
            (1, "a".into()),
            (2, String::new()),
            (3, "x\0".into()),
            (4, "β".into()),
        ],
        limits,
    )?;
    let body = encode_raft_membership(
        s.group(),
        s.recovery(),
        EntryStamp::Unstamped,
        &value,
        limits,
        6843,
    )?;
    assert_eq!(body.as_bytes().len(), 134);
    assert_eq!(
        body.digest().as_bytes().as_slice(),
        fixtures::hex("ec550282ecf2cb11f71e4f9a9aa2dac6318ab8772c0da4c432d9c30175a3edbe")?
    );
    Ok(body)
}

fn assert_membership_authority(
    state: &mut RedbStateStore,
    body: &PreparedBytes,
    applied: bool,
) -> TestResult {
    let s = fixtures::scope()?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        fixtures::MIB,
        fixtures::MIB,
    )?;
    let control = tx.control()?;
    let member = tx.applied_membership(RaftMembershipLimits::default())?;
    if applied {
        let entry = EntryIdentity::new(fixtures::id(1, 7, 0), body.digest());
        assert_eq!(
            control,
            ControlState::restored(fixtures::clock(0)?, entry, EntryStamp::Unstamped)?
        );
        assert_eq!(member.entry(), Some(entry));
        assert_eq!(member.stamp(), Some(EntryStamp::Unstamped));
        assert_eq!(
            member.membership().configs(),
            [vec![1, 2], vec![2, 3], vec![1, 2]]
        );
        assert_eq!(
            member.membership().nodes(),
            [
                (1, "a".into()),
                (2, String::new()),
                (3, "x\0".into()),
                (4, "β".into())
            ]
        );
    } else {
        assert_eq!(control, ControlState::initial(fixtures::clock(0)?));
        assert!(member.entry().is_none());
        assert!(member.stamp().is_none());
        let configs: &[Vec<u64>] = &[];
        let nodes: &[(u64, String)] = &[];
        assert_eq!(member.membership().configs(), configs);
        assert_eq!(member.membership().nodes(), nodes);
    }
    drop(tx);
    fixtures::assert_authority(state, control, false)
}

#[test]
fn control_apply_uses_actual_guard_limits() -> TestResult {
    let s = fixtures::scope()?;
    let limits = RaftMembershipLimits::default();
    let body = membership_frame()?;
    // J646 + fixed3072 + actual cumulative R/C; C includes the single O211.
    for (r, c, w, error) in [
        (1024, 342, 5084, None),
        (1024, 342, 5083, Some(StateTransactionError::Limit)),
        (1024, 341, 5084, Some(StateTransactionError::Limit)),
        (1023, 342, 5084, Some(StateTransactionError::Limit)),
        (1024, 65536, 70278, None),
        (2048, 65536, 70278, Some(StateTransactionError::Limit)),
        (1024, 65537, 70278, Some(StateTransactionError::Limit)),
        (
            usize::MAX,
            342,
            usize::MAX,
            Some(StateTransactionError::Limit),
        ),
        (1024, 342, 0, Some(StateTransactionError::InvalidLimits)),
    ] {
        let dir = fixtures::directory("control-apply-caps")?;
        let (mut log, mut state) = fixtures::create_pair(&dir)?;
        log.append(vec![RaftLogEntry::new(
            fixtures::id(1, 7, 0),
            body.as_bytes().to_vec(),
        )])?;
        log.save_committed(fixtures::id(1, 7, 0))?;
        let tx = state.begin_apply(
            s.group(),
            s.recovery(),
            AdmissionLimits::default(),
            100,
            r,
            c,
        )?;
        assert_eq!(tx.owner_limits(), (r, c));
        let result = apply_control_entry(
            tx,
            s.group(),
            s.recovery(),
            body.as_bytes(),
            fixtures::id(1, 7, 0),
            limits,
            w,
        );
        match (result, error) {
            (Ok(control), None) => assert_eq!(
                control.last_entry(),
                Some(EntryIdentity::new(fixtures::id(1, 7, 0), body.digest()))
            ),
            (Err(actual), Some(expected)) => assert_eq!(actual, expected),
            _ => return Err(format!("wrong control apply result: R{r}/C{c}/W{w}").into()),
        }
        assert_membership_authority(&mut state, &body, error.is_none())?;
        drop((log, state));
        let (log, state) = fixtures::open_pair(&dir)?;
        let (mut log, mut state) = verify_checkpoint(
            log,
            state,
            s.group(),
            s.recovery(),
            CheckpointJoinLimits::default(),
        )?;
        assert_membership_authority(&mut state, &body, error.is_none())?;
        let saved = log.state()?;
        assert_eq!(saved.committed(), Some(fixtures::id(1, 7, 0)));
        fixtures::assert_log(&mut log, saved, &[&body])?;
        drop((log, state));
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn control_read_uses_actual_guard_limits() -> TestResult {
    let dir = fixtures::directory("control-read-caps")?;
    let (mut log, mut state) = fixtures::create_pair(&dir)?;
    let s = fixtures::scope()?;
    let body = membership_frame()?;
    let limits = RaftMembershipLimits::default();
    log.append(vec![RaftLogEntry::new(
        fixtures::id(1, 7, 0),
        body.as_bytes().to_vec(),
    )])?;
    log.save_committed(fixtures::id(1, 7, 0))?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        1024,
        342,
    )?;
    let expected = apply_control_entry(
        tx,
        s.group(),
        s.recovery(),
        body.as_bytes(),
        fixtures::id(1, 7, 0),
        limits,
        5084,
    )?;
    // Returned control512 + U4661 share R; read peak is3072 + actual R/C.
    for (r, c, w, error) in [
        (5173, 13, 8258, None),
        (5173, 13, 8257, Some(StateTransactionError::Limit)),
        (5172, 13, 8258, Some(StateTransactionError::Limit)),
        (5173, 12, 8258, Some(StateTransactionError::Limit)),
        (5174, 13, 8258, Some(StateTransactionError::Limit)),
        (5173, 14, 8258, Some(StateTransactionError::Limit)),
        (
            usize::MAX,
            13,
            usize::MAX,
            Some(StateTransactionError::Limit),
        ),
        (5173, 13, 0, Some(StateTransactionError::InvalidLimits)),
    ] {
        let tx = state.begin_apply(
            s.group(),
            s.recovery(),
            AdmissionLimits::default(),
            100,
            r,
            c,
        )?;
        assert_eq!(tx.owner_limits(), (r, c));
        let result = read_applied_state(&tx, limits, w);
        match (result, error) {
            (Ok((control, member)), None) => {
                assert_eq!(control, expected);
                assert_eq!(member.entry(), expected.last_entry());
                assert_eq!(member.stamp(), Some(EntryStamp::Unstamped));
                assert_eq!(
                    member.membership().configs(),
                    [vec![1, 2], vec![2, 3], vec![1, 2]]
                );
                assert_eq!(
                    member.membership().nodes(),
                    [
                        (1, "a".into()),
                        (2, String::new()),
                        (3, "x\0".into()),
                        (4, "β".into())
                    ]
                );
            }
            (Err(actual), Some(expected)) => assert_eq!(actual, expected),
            _ => return Err(format!("wrong control read result: R{r}/C{c}/W{w}").into()),
        }
        drop(tx);
        assert_membership_authority(&mut state, &body, true)?;
    }
    drop((log, state));
    let (_, mut state) = fixtures::open_pair(&dir)?;
    assert_membership_authority(&mut state, &body, true)?;
    drop(state);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
