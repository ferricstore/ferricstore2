use super::{CommittedRecoveryError as Error, TestResult, assert_refusal, fixtures, limits};
use wf_consensus::recovery::{CheckpointJoinLimits, verify_checkpoint};
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp};
use wf_ports::consensus::RaftMembershipLimits;
use wf_ports::log::{RaftLogStore, encode_raft_blank};
use wf_storage::state::{key, value};
use wf_types::identity::GroupId;

#[test]
fn committed_recovery_audits_latest_membership_and_every_historical_clock() -> TestResult {
    for case in [
        "stale",
        "false-none",
        "hlc",
        "wall",
        "foreign",
        "unsupported",
    ] {
        let dir = fixtures::directory(case)?;
        forked_history(&dir, case)?;
        // The weaker predecessor deliberately accepts this EXACT same A point.
        // That isolates the new full-prefix obligation from ordinary A matching.
        let (log, state) = fixtures::open_pair(&dir)?;
        let s = fixtures::scope()?;
        drop(verify_checkpoint(
            log,
            state,
            s.group(),
            s.recovery(),
            CheckpointJoinLimits::default(),
        )?);
        let expected = match case {
            "stale" | "false-none" => Error::MembershipMismatch,
            "hlc" | "wall" => Error::ClockMismatch,
            "foreign" => {
                Error::Membership(wf_ports::consensus::RaftMembershipError::NamespaceMismatch)
            }
            _ => Error::UnsupportedEntry,
        };
        assert_refusal(&dir, limits()?, expected)?;
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

fn forked_history(dir: &std::path::Path, case: &str) -> TestResult {
    let (mut log, mut state) = fixtures::create_pair(dir)?;
    let blank = fixtures::blank(EntryStamp::Unstamped)?;
    let first = fixtures::member(EntryStamp::Unstamped, 4)?;
    let second = fixtures::member(EntryStamp::Unstamped, 5)?;
    let (actual, alternative) = match case {
        "hlc" => (
            fixtures::blank(EntryStamp::Stamped(fixtures::clock(101, 0, 80)?))?
                .as_bytes()
                .to_vec(),
            fixtures::blank(EntryStamp::Stamped(fixtures::clock(100, 0, 80)?))?
                .as_bytes()
                .to_vec(),
        ),
        "wall" => (
            fixtures::blank(EntryStamp::Stamped(fixtures::clock(100, 0, 90)?))?
                .as_bytes()
                .to_vec(),
            fixtures::blank(EntryStamp::Stamped(fixtures::clock(100, 0, 80)?))?
                .as_bytes()
                .to_vec(),
        ),
        "foreign" => {
            let s = fixtures::scope()?;
            (
                encode_raft_blank(
                    &GroupId::new(b"other".to_vec())?,
                    s.recovery(),
                    EntryStamp::Unstamped,
                    RaftMembershipLimits::default(),
                    fixtures::MIB,
                )?
                .as_bytes()
                .to_vec(),
                blank.as_bytes().to_vec(),
            )
        }
        "unsupported" => (
            b"unrecognized historical body".to_vec(),
            blank.as_bytes().to_vec(),
        ),
        _ => (first.as_bytes().to_vec(), blank.as_bytes().to_vec()),
    };
    fixtures::append(&mut log, 0, &actual)?;
    fixtures::apply_control(
        &mut state,
        0,
        if case == "stale" {
            first.as_bytes()
        } else {
            &alternative
        },
    )?;
    let final_index = if case == "stale" {
        fixtures::append(&mut log, 1, second.as_bytes())?;
        fixtures::apply_control(&mut state, 1, blank.as_bytes())?;
        2
    } else {
        1
    };
    fixtures::append(&mut log, final_index, blank.as_bytes())?;
    let control = fixtures::apply_control(&mut state, final_index, blank.as_bytes())?;
    assert_eq!(
        control.last_entry(),
        Some(EntryIdentity::new(
            fixtures::at(final_index),
            blank.digest()
        ))
    );
    let suffix = fixtures::frame(
        5,
        b"must-not-apply",
        102,
        vec![(b"i", fixtures::create_op())],
    )?;
    fixtures::append(&mut log, final_index + 1, suffix.as_bytes())?;
    log.save_committed(fixtures::at(final_index + 1))?;
    drop((log, state));
    // Each store independently accepts its valid history; no counterfeit checksums.
    drop(fixtures::open_pair(dir)?);
    Ok(())
}

#[test]
fn committed_recovery_checks_original_stamp_and_complete_normal_body() -> TestResult {
    for case in ["stamp", "normal-eof", "normal-intent"] {
        let dir = fixtures::directory(case)?;
        let (mut log, mut state) = fixtures::create_pair(&dir)?;
        let blank = fixtures::blank(EntryStamp::Unstamped)?;
        if case == "stamp" {
            fixtures::append(&mut log, 0, blank.as_bytes())?;
            let control = fixtures::apply_control(&mut state, 0, blank.as_bytes())?;
            log.save_committed(fixtures::at(0))?;
            drop((log, state));
            let s = fixtures::scope()?;
            let changed = ControlState::restored(
                control.time(),
                control.last_entry().ok_or("missing A")?,
                EntryStamp::Stamped(fixtures::clock(0, 0, 0)?),
            )?;
            replace_control(
                &dir,
                &key::encode_control_key(s.group(), s.recovery(), fixtures::MIB)?,
                &value::encode_control(s.group(), s.recovery(), changed, fixtures::MIB)?,
            )?;
            assert_refusal(&dir, limits()?, Error::StampMismatch)?;
        } else {
            let frame = fixtures::frame(5, b"old", 1, vec![(b"i", fixtures::create_op())])?;
            let mut malformed = frame.as_bytes().to_vec();
            if case == "normal-eof" {
                malformed.push(0);
            } else {
                malformed[31] ^= 0x40;
            }
            fixtures::append(&mut log, 0, &malformed)?;
            let old = fixtures::blank(EntryStamp::Stamped(fixtures::clock(100, 0, 100)?))?;
            fixtures::apply_control(&mut state, 0, old.as_bytes())?;
            fixtures::append(&mut log, 1, blank.as_bytes())?;
            fixtures::apply_control(&mut state, 1, blank.as_bytes())?;
            log.save_committed(fixtures::at(1))?;
            drop((log, state));
            let expected = if case == "normal-eof" {
                wf_ports::log::RecordBatchApplyError::Encoding(
                    wf_ports::log::CodecError::TrailingBytes,
                )
            } else {
                wf_ports::log::RecordBatchApplyError::IntentMismatch
            };
            assert_refusal(
                &dir,
                limits()?,
                Error::Normal(wf_storage::transaction::RecordBatchTransactionError::Apply(
                    expected,
                )),
            )?;
        }
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

fn replace_control(dir: &std::path::Path, key: &[u8], value: &[u8]) -> TestResult {
    let database = redb::Builder::new().open(dir.join("state.redb"))?;
    let mut write = database.begin_write()?;
    write.set_durability(redb::Durability::Immediate)?;
    {
        let mut table = write.open_table(redb::TableDefinition::<&[u8], &[u8]>::new(
            "wf_state_rows_v1",
        ))?;
        assert!(table.insert(key, value)?.is_some());
    }
    write.commit()?;
    Ok(())
}

#[test]
fn committed_recovery_binds_full_checkpoint_identity_and_actual_body() -> TestResult {
    for case in ["term", "node", "body", "ahead", "uncommitted"] {
        let dir = fixtures::directory(case)?;
        let (mut log, mut state) = fixtures::create_pair(&dir)?;
        let blank = fixtures::blank(EntryStamp::Unstamped)?;
        let first = match case {
            "term" => wf_types::version::LogIdentity::new(
                2,
                7,
                wf_types::version::AppliedPosition::new(0),
            ),
            "node" => wf_types::version::LogIdentity::new(
                1,
                8,
                wf_types::version::AppliedPosition::new(0),
            ),
            _ => fixtures::at(0),
        };
        if case == "body" {
            let actual = fixtures::frame(5, b"a", 1, vec![(b"i", fixtures::create_op())])?;
            let other = fixtures::frame(5, b"b", 1, vec![(b"j", fixtures::create_op())])?;
            fixtures::append(&mut log, 0, actual.as_bytes())?;
            drop(fixtures::apply_normal(&mut state, 0, other.as_bytes(), 5)?);
        } else {
            log.append(vec![wf_ports::log::RaftLogEntry::new(
                first,
                blank.as_bytes().to_vec(),
            )])?;
            fixtures::apply_control(&mut state, 0, blank.as_bytes())?;
        }
        if case != "uncommitted" {
            log.save_committed(first)?;
        }
        if matches!(case, "term" | "node" | "body") {
            let end = wf_types::version::LogIdentity::new(
                3,
                7,
                wf_types::version::AppliedPosition::new(1),
            );
            log.append(vec![wf_ports::log::RaftLogEntry::new(
                end,
                blank.as_bytes().to_vec(),
            )])?;
            log.save_committed(end)?;
        }
        if case == "ahead" {
            fixtures::apply_control(&mut state, 1, blank.as_bytes())?;
        }
        drop((log, state));
        let expected = match case {
            "term" | "node" => Error::IdentityMismatch,
            "body" => Error::BodyMismatch,
            _ => Error::IncompatibleCheckpoint,
        };
        assert_refusal(&dir, limits()?, expected)?;
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}
