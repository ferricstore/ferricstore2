use super::{TestResult, fixtures, limits, recover_committed};
use wf_domain::workflow::{
    control::{EntryIdentity, EntryStamp, plan_control},
    request::RetainedStatus,
};
use wf_ports::log::{PreparedBytes, RaftLogStore};
use wf_storage::transaction::RecordBatchTransactionOutcome as Outcome;

#[test]
fn committed_recovery_replays_mixed_entries_and_preserves_original_outcomes() -> TestResult {
    let frames = mixed_frames()?;
    let expected = fixtures::directory("mixed-reference")?;
    let (log, mut state) = fixtures::create_pair(&expected)?;
    let mut control = fixtures::applied(&mut state)?.0;
    for (index, (profile, frame, stamp)) in frames.iter().enumerate() {
        let index = u64::try_from(index)?;
        let planned = plan_control(
            control,
            EntryIdentity::new(fixtures::at(index), frame.digest()),
            *stamp,
        )?
        .next();
        control = if *profile == 0 {
            fixtures::apply_control(&mut state, index, frame.as_bytes())?
        } else {
            let outcome = fixtures::apply_normal(&mut state, index, frame.as_bytes(), *profile)?;
            assert_classification(index, &outcome)?;
            match outcome {
                Outcome::Retained { control, .. }
                | Outcome::IntentConflict(control)
                | Outcome::Expired(control)
                | Outcome::LogReplay(control) => control,
            }
        };
        assert_eq!(control, planned);
    }
    let (_, membership) = fixtures::applied(&mut state)?;
    assert_eq!(
        membership.entry(),
        Some(EntryIdentity::new(fixtures::at(10), frames[10].1.digest()))
    );
    assert_eq!(
        membership.membership().configs(),
        [vec![1, 2], vec![2, 3], vec![1, 2]]
    );
    assert_eq!(membership.membership().nodes()[3], (5, "β".into()));
    drop((log, state));
    let rows = fixtures::state_rows(&expected)?;
    let meta = fixtures::state_meta(&expected)?;
    for (family, count) in [
        (0, 1),
        (1, 2),
        (2, 4),
        (3, 1),
        (4, 1),
        (5, 3),
        (6, 1),
        (7, 1),
    ] {
        assert_eq!(
            rows.keys()
                .filter(|key| key.get(6) == Some(&family))
                .count(),
            count
        );
    }
    // Independent packed keys: i's member is distinct from its derived head due.
    let fifo = fixtures::hex(
        "4657504b0001046700007a00007400007200007100007000000100000000000000000000000200000000690000",
    )?;
    let due = fixtures::hex(
        "4657504b0001036700007a000074000002000000000000000a690000700000710000720000",
    )?;
    assert_eq!(rows.get(&fifo).map(Vec::as_slice), Some(&[][..]));
    assert_eq!(rows.get(&due).map(Vec::as_slice), Some(&[1][..]));
    for already_applied in [0, 2, 6, 11] {
        replay_from_prefix(&frames, already_applied, control, &rows, &meta)?;
    }
    std::fs::remove_dir_all(expected)?;
    Ok(())
}

fn mixed_frames() -> TestResult<Vec<(u16, PreparedBytes, EntryStamp)>> {
    let a = || fixtures::frame(5, b"a", 10, vec![(b"i", fixtures::create_op())]);
    Ok(vec![
        (
            0,
            fixtures::blank(EntryStamp::Unstamped)?,
            EntryStamp::Unstamped,
        ),
        (
            0,
            fixtures::member(EntryStamp::Stamped(fixtures::clock(10, 0, 10)?), 4)?,
            EntryStamp::Stamped(fixtures::clock(10, 0, 10)?),
        ),
        (5, a()?, EntryStamp::Stamped(fixtures::clock(10, 0, 10)?)),
        (5, a()?, EntryStamp::Stamped(fixtures::clock(10, 0, 10)?)),
        (
            5,
            fixtures::frame(5, b"a", 10, vec![(b"j", fixtures::create_op())])?,
            EntryStamp::Stamped(fixtures::clock(10, 0, 10)?),
        ),
        (
            6,
            fixtures::frame(
                6,
                b"b",
                11,
                vec![(b"j", fixtures::create_op()), (b"i", fixtures::create_op())],
            )?,
            EntryStamp::Stamped(fixtures::clock(11, 0, 11)?),
        ),
        (
            1,
            fixtures::frame(1, b"c", 12, vec![(b"j", fixtures::cancel_op())])?,
            EntryStamp::Stamped(fixtures::clock(12, 0, 12)?),
        ),
        (
            5,
            fixtures::frame(5, b"reject", 13, vec![(b"i", fixtures::create_op())])?,
            EntryStamp::Stamped(fixtures::clock(13, 0, 13)?),
        ),
        (
            0,
            fixtures::blank(EntryStamp::Stamped(fixtures::clock(2011, 0, 2011)?))?,
            EntryStamp::Stamped(fixtures::clock(2011, 0, 2011)?),
        ),
        (5, a()?, EntryStamp::Stamped(fixtures::clock(10, 0, 10)?)),
        (
            0,
            fixtures::member(EntryStamp::Unstamped, 5)?,
            EntryStamp::Unstamped,
        ),
    ])
}

fn assert_classification(index: u64, outcome: &Outcome) -> TestResult {
    match index {
        2 | 3 => {
            let Outcome::Retained { outcome, .. } = outcome else {
                return Err("missing original reply".into());
            };
            assert_eq!(outcome.status(), RetainedStatus::Accepted);
            assert_eq!(outcome.original_position(), fixtures::at(2));
            assert_eq!(outcome.original_time().get(), 10);
            assert_eq!(outcome.expires_at().get(), 1010);
            assert_eq!(outcome.response(), [0]);
        }
        4 => assert!(matches!(outcome, Outcome::IntentConflict(_))),
        5..=7 => {
            let Outcome::Retained { outcome, .. } = outcome else {
                return Err("missing fresh reply".into());
            };
            assert_eq!(outcome.original_position(), fixtures::at(index));
            assert_eq!(outcome.original_time().get(), index + 6);
            let rejected = RetainedStatus::IndexedRejected(
                wf_domain::workflow::indexed::IndexRejection::Batch(
                    wf_domain::workflow::batch::BatchRejection::Domain(
                        wf_domain::workflow::DomainError::AlreadyExists,
                    ),
                ),
            );
            assert_eq!(
                outcome.status(),
                if index == 7 {
                    rejected
                } else {
                    RetainedStatus::Accepted
                }
            );
            assert_eq!(
                outcome.response(),
                match index {
                    5 => &[2, 0, 0, 0, 2, 0, 1, 0, 1][..],
                    6 => &[0][..],
                    _ => &[1, 0, 1][..],
                }
            );
        }
        9 => assert!(matches!(outcome, Outcome::Expired(_))),
        _ => return Err("unclassified normal fixture".into()),
    }
    Ok(())
}

fn replay_from_prefix(
    frames: &[(u16, PreparedBytes, EntryStamp)],
    prefix: usize,
    final_control: wf_domain::workflow::control::ControlState,
    rows: &fixtures::Rows,
    meta: &fixtures::Rows,
) -> TestResult {
    let dir = fixtures::directory("mixed")?;
    let (mut log, mut state) = fixtures::create_pair(&dir)?;
    for (index, (profile, frame, _)) in frames.iter().enumerate() {
        let index = u64::try_from(index)?;
        fixtures::append(&mut log, index, frame.as_bytes())?;
        if usize::try_from(index)? < prefix {
            if *profile == 0 {
                fixtures::apply_control(&mut state, index, frame.as_bytes())?;
            } else {
                drop(fixtures::apply_normal(
                    &mut state,
                    index,
                    frame.as_bytes(),
                    *profile,
                )?);
            }
        }
    }
    fixtures::append(&mut log, 11, b"poisoned uncommitted body")?;
    log.save_committed(fixtures::at(10))?;
    let old_log = log.state()?;
    drop((log, state));
    let log_rows = fixtures::log_rows(&dir)?;
    let log_meta = fixtures::log_meta(&dir)?;
    for _ in 0..2 {
        let (log, state) = fixtures::open_pair(&dir)?;
        let s = fixtures::scope()?;
        let (mut log, mut state) =
            recover_committed(log, state, s.group(), s.recovery(), limits()?)?;
        assert_eq!(log.state()?, old_log);
        assert_eq!(fixtures::applied(&mut state)?.0, final_control);
        drop((log, state));
        assert_eq!(fixtures::state_rows(&dir)?, *rows);
        assert_eq!(fixtures::state_meta(&dir)?, *meta);
        assert_eq!(fixtures::log_rows(&dir)?, log_rows);
        assert_eq!(fixtures::log_meta(&dir)?, log_meta);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn committed_recovery_accepts_empty_and_ignores_uncommitted_opaque_tail() -> TestResult {
    for append in [false, true] {
        let dir = fixtures::directory("empty")?;
        let (mut log, state) = fixtures::create_pair(&dir)?;
        if append {
            fixtures::append(&mut log, 0, b"uncommitted unsupported")?;
        }
        let expected = log.state()?;
        drop((log, state));
        let rows = fixtures::state_rows(&dir)?;
        let meta = fixtures::state_meta(&dir)?;
        let logs = fixtures::log_rows(&dir)?;
        let (log, state) = fixtures::open_pair(&dir)?;
        let s = fixtures::scope()?;
        let (mut log, mut state) =
            recover_committed(log, state, s.group(), s.recovery(), limits()?)?;
        assert_eq!(log.state()?, expected);
        let (control, member) = fixtures::applied(&mut state)?;
        assert_eq!(control.last_entry(), None);
        assert_eq!(member.entry(), None);
        drop((log, state));
        assert_eq!(fixtures::state_rows(&dir)?, rows);
        assert_eq!(fixtures::state_meta(&dir)?, meta);
        assert_eq!(fixtures::log_rows(&dir)?, logs);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}
