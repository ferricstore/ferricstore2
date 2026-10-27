//! Actual all-mode/retained replay and intermediate independent provenance.

use wf_domain::workflow::Transition;
use wf_domain::workflow::request::RetainedStatus;
use wf_types::version::Fence;

use super::*;

#[test]
fn stored_recovery_replays_all_modes_and_retained_outcomes() -> TestResult {
    let profile =
        limits()?.with_workflow_value_limits(1_024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB)?;
    for (mode, payload) in [
        (1, b"raw".as_slice()),
        (5, b"FSV2\x01escaped"),
        (6, b"FSV2\x02\x83\x61\x07"),
        (7, b"FSV2\x02bad-etf"),
    ] {
        let dir = f::directory("stored-node-modes")?;
        let (mut log, state, policy) = seed(&dir)?;
        let frame = encode(
            mode,
            b"entry",
            100,
            vec![item(mode, b"i", 100, Some(payload))?],
            &policy,
        )?;
        f::append(&mut log, 1, frame.as_bytes())?;
        f::append(&mut log, 2, frame.as_bytes())?;
        let legacy = v::encode(&v::batch(
            &f::scope()?,
            5,
            b"legacy",
            200,
            vec![v::create(b"legacy", 200, b"FSV2legacy")?],
            v::copy_policy(&policy),
        )?)?;
        f::append(&mut log, 3, legacy.as_bytes())?;
        f::append(&mut log, 4, b"uncommitted opaque tail")?;
        log.save_committed(f::at(3))?;
        drop((log, state));
        let result = recover(&dir, &profile);
        assert!(
            result.is_ok(),
            "healthy stored Node replay: {:?}",
            result.as_ref().err()
        );
        let (mut log, mut state) = result?;
        assert_eq!(v::control(&mut state)?.position(), f::at(3).index());
        let original = v::outcome(&mut state, b"entry")?;
        assert_eq!(original.intent().version(), 9);
        assert_eq!(original.status(), RetainedStatus::Accepted);
        assert_eq!(
            original.response_version(),
            if mode == 1 { 8 } else { u16::from(mode) }
        );
        assert_eq!(original.original_position(), f::at(1));
        assert_eq!(original.original_time().get(), 100);
        assert_eq!(v::outcome(&mut state, b"legacy")?.intent().version(), 8);
        assert_eq!(log.state()?.last_log(), Some(f::at(4)));
        assert_eq!(log.state()?.committed(), Some(f::at(3)));
        assert_payload(&state, b"i", 1, payload)?;
        assert_payload(&state, b"legacy", 1, b"FSV2\x01FSV2legacy")?;
        v::binding(
            &state,
            b"i",
            None,
            (1_u128 << 32, [Some(1), None, None], None),
            &policy,
        )?;
        drop((log, state));
        let before = v::cohort(&dir)?;
        let logs = f::log_rows(&dir)?;
        let (log, mut state) = recover(&dir, &profile)?;
        assert_eq!(v::outcome(&mut state, b"entry")?, original);
        drop((log, state));
        assert_eq!(v::cohort(&dir)?, before);
        assert_eq!(f::log_rows(&dir)?, logs);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn stored_recovery_independent_source_holes_keep_every_history() -> TestResult {
    let dir = f::directory("stored-node-holes")?;
    let (mut log, state, policy) = seed(&dir)?;
    let transition = Operation::Transition(Transition {
        from: b"q".to_vec(),
        to: b"next".to_vec(),
        fence: Fence::new(0),
        lease_token: None,
        run_at: None,
        payload: Some(b"FSV2\x02bad-etf".to_vec()),
    });
    let frame = encode(
        6,
        b"holes",
        100,
        vec![
            item(6, b"i", 100, Some(b"FSV2\x02\x83\x61\x01"))?,
            item(6, b"i", 100, Some(b"rejected"))?,
            v::item(b"i", 100, transition)?,
            item(6, b"j", 100, Some(b"FSV2\x01raw"))?,
        ],
        &policy,
    )?;
    f::append(&mut log, 1, frame.as_bytes())?;
    log.save_committed(f::at(1))?;
    drop((log, state));
    let result = recover(&dir, &limits()?);
    assert!(
        result.is_ok(),
        "healthy stored Node replay: {:?}",
        result.as_ref().err()
    );
    let (log, mut state) = result?;
    assert_eq!(v::outcome(&mut state, b"holes")?.intent().version(), 9);
    for (id, event, incarnation, version) in [
        (b"i".as_slice(), None, 1_u128 << 32, 2),
        (b"i", Some("100-1"), 1_u128 << 32, 1),
        (b"i", Some("100-2"), 1_u128 << 32, 2),
        (b"j", None, (1_u128 << 32) + 2, 1),
    ] {
        v::binding(
            &state,
            id,
            event,
            (incarnation, [Some(version), None, None], None),
            &policy,
        )?;
    }
    assert_payload(&state, b"i", 1, b"FSV2\x02\x83\x61\x01")?;
    assert_payload(&state, b"i", 2, b"FSV2\x02bad-etf")?;
    assert_payload(&state, b"j", 1, b"FSV2\x01raw")?;
    drop((log, state));
    let rows = v::cohort(&dir)?;
    for (family, count) in [(8, 3), (9, 3), (10, 2), (11, 3)] {
        assert_eq!(
            rows[1]
                .keys()
                .filter(|key| key.get(6) == Some(&family))
                .count(),
            count
        );
    }
    assert!(
        rows[1]
            .iter()
            .filter(|(key, _)| key.get(6) == Some(&9))
            .all(|(_, value)| value.starts_with(b"FWVA\0\x02"))
    );
    drop(recover(&dir, &limits()?)?);
    assert_eq!(v::cohort(&dir)?, rows);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
