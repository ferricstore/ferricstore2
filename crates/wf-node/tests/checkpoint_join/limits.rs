use super::{TestResult, fixtures};
use wf_consensus::recovery::{CheckpointJoinError, CheckpointJoinLimits, verify_checkpoint};
use wf_ports::log::{RaftLogEntry, RaftLogStore};
use wf_ports::state::StateTransactionError;

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
