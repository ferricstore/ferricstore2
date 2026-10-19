use super::{TestResult, fixtures};
use wf_consensus::recovery::{CheckpointJoinError, CheckpointJoinLimits, verify_checkpoint};
use wf_ports::log::{RaftLogEntry, RaftLogStore, RaftVote};
use wf_types::identity::{GroupId, RecoveryId};

#[test]
fn checkpoint_join_accepts_empty_and_verified_prefixes() -> TestResult {
    for phase in 0..6 {
        let dir = fixtures::directory("valid")?;
        let (mut log, mut state) = fixtures::create_pair(&dir)?;
        let frame = fixtures::frame(b"i", b"q")?;
        let later = fixtures::frame(b"j", b"j")?;
        if phase >= 1 {
            log.save_vote(RaftVote::new(1, 7, true))?;
        }
        if phase >= 2 {
            log.append(vec![RaftLogEntry::new(
                fixtures::id(1, 7, 0),
                frame.as_bytes().to_vec(),
            )])?;
        }
        if phase >= 3 {
            log.save_committed(fixtures::id(1, 7, 0))?;
        }
        if phase >= 4 {
            fixtures::apply(&mut state, &frame, fixtures::id(1, 7, 0))?;
        }
        if phase >= 5 {
            log.append(vec![RaftLogEntry::new(
                fixtures::id(2, 0, 1),
                later.as_bytes().to_vec(),
            )])?;
            log.save_committed(fixtures::id(2, 0, 1))?;
        }
        let meta = log.state()?;
        let expected = fixtures::control(&mut state)?;
        drop((log, state));
        for _ in 0..2 {
            let (log, state) = fixtures::open_pair(&dir)?;
            let s = fixtures::scope()?;
            let (mut log, mut state) = verify_checkpoint(
                log,
                state,
                s.group(),
                s.recovery(),
                CheckpointJoinLimits::default(),
            )?;
            fixtures::assert_authority(&mut state, expected, phase >= 4)?;
            let bodies = match phase {
                0 | 1 => vec![],
                5 => vec![&frame, &later],
                _ => vec![&frame],
            };
            fixtures::assert_log(&mut log, meta, &bodies)?;
        }
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn checkpoint_join_rejects_identity_and_body_mismatches() -> TestResult {
    for case in [
        "term",
        "node",
        "ahead",
        "uncommitted",
        "point-term",
        "point-node",
        "body",
        "group",
        "recovery",
        "state-group",
    ] {
        reject_case(case)?;
    }
    Ok(())
}

fn reject_case(case: &str) -> TestResult {
    let dir = fixtures::directory(case)?;
    let (mut log, mut state) = fixtures::create_pair(&dir)?;
    let frame = fixtures::frame(b"i", b"q")?;
    let other = fixtures::frame(b"j", b"j")?;
    let first = match case {
        "term" | "point-term" => fixtures::id(2, 7, 0),
        "node" | "point-node" => fixtures::id(1, 8, 0),
        _ => fixtures::id(1, 7, 0),
    };
    log.append(vec![RaftLogEntry::new(
        first,
        if case == "body" {
            other.as_bytes()
        } else {
            frame.as_bytes()
        }
        .to_vec(),
    )])?;
    if case != "uncommitted" {
        log.save_committed(first)?;
    }
    if case.starts_with("point-") {
        log.append(vec![RaftLogEntry::new(
            fixtures::id(3, 0, 1),
            other.as_bytes().to_vec(),
        )])?;
        log.save_committed(fixtures::id(3, 0, 1))?;
    }
    fixtures::apply(&mut state, &frame, fixtures::id(1, 7, 0))?;
    if case == "ahead" {
        fixtures::apply(&mut state, &other, fixtures::id(1, 7, 1))?;
    }
    let expected = fixtures::control(&mut state)?;
    let meta = log.state()?;
    let s = fixtures::scope()?;
    let group = GroupId::new(match case {
        "group" => b"wrong".to_vec(),
        "state-group" => b"other".to_vec(),
        _ => b"g".to_vec(),
    })?;
    let recovery = RecoveryId::new(if case == "recovery" {
        b"wrong".to_vec()
    } else {
        b"z".to_vec()
    })?;
    if case == "state-group" {
        drop(log);
        log = wf_storage::log::RedbLogStore::create(
            &dir.join("other.redb"),
            GroupId::new(b"other".to_vec())?,
            s.recovery().clone(),
            wf_ports::log::RaftLogLimits::default(),
        )?;
    }
    let Err(error) = verify_checkpoint(
        log,
        state,
        &group,
        &recovery,
        CheckpointJoinLimits::default(),
    ) else {
        return Err(format!("unexpected valid join: {case}").into());
    };
    assert_eq!(error, expected_error(case), "{case}");
    // Reacquire both locks immediately after the consuming failure.
    let (mut log, mut state) = fixtures::open_pair(&dir)?;
    assert_eq!(log.state()?, meta);
    assert_eq!(fixtures::control(&mut state)?, expected);
    if case != "ahead" {
        fixtures::assert_authority(&mut state, expected, true)?;
    }
    if case == "state-group" {
        drop(wf_storage::log::RedbLogStore::open(
            &dir.join("other.redb"),
            group,
            recovery,
            wf_ports::log::RaftLogLimits::default(),
        )?);
    }
    let rows = log.read_page(std::ops::Bound::Unbounded, std::ops::Bound::Unbounded)?;
    assert_eq!(rows.len(), if case.starts_with("point-") { 2 } else { 1 });
    assert_eq!(rows[0].id(), first);
    assert_eq!(
        rows[0].body(),
        if case == "body" {
            other.as_bytes()
        } else {
            frame.as_bytes()
        }
    );
    if case.starts_with("point-") {
        assert_eq!(rows[1].id(), fixtures::id(3, 0, 1));
        assert_eq!(rows[1].body(), other.as_bytes());
    }
    drop((log, state));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn expected_error(case: &str) -> CheckpointJoinError {
    match case {
        "group" | "recovery" => CheckpointJoinError::NamespaceMismatch,
        "state-group" => {
            CheckpointJoinError::State(wf_ports::state::StateTransactionError::InvalidChanges)
        }
        "point-term" | "point-node" => CheckpointJoinError::IdentityMismatch,
        "body" => CheckpointJoinError::BodyMismatch,
        _ => CheckpointJoinError::IncompatibleCheckpoint,
    }
}
