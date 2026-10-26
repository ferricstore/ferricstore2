//! The retained ordering vector follows accepted entries, not source positions.

use wf_domain::workflow::{Operation, Transition};
use wf_ports::blob::OriginalValueRefKind;
use wf_ports::log::RaftLogStore;
use wf_types::version::Fence;

use super::{TestResult, f, fixtures as v, recover_committed};

#[test]
fn value_recovery_independent_holes_preserve_every_intermediate_value() -> TestResult {
    let enabled =
        v::limits()?.with_workflow_value_limits(1_024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB);
    assert!(
        enabled.is_ok(),
        "explicit value recovery opt-in must succeed"
    );
    let limits = enabled?;
    let dir = f::directory("value-independent-holes")?;
    let (mut log, mut state) = v::pair(&dir)?;
    let policy = v::install(&mut log, &mut state, true, None, 0, 10)?;
    let transition = Operation::Transition(Transition {
        from: b"q".to_vec(),
        to: b"next".to_vec(),
        fence: Fence::new(0),
        lease_token: None,
        run_at: None,
        payload: Some(b"B".to_vec()),
    });
    let batch = v::batch(
        &f::scope()?,
        6,
        b"holes",
        100,
        vec![
            v::create(b"i", 100, b"A")?,
            v::create(b"i", 100, b"rejected")?,
            v::item(b"i", 100, transition)?,
            v::create(b"j", 100, b"C")?,
            v::item(
                b"i",
                100,
                Operation::Transition(Transition {
                    from: b"q".to_vec(),
                    to: b"next".to_vec(),
                    fence: Fence::new(0),
                    lease_token: None,
                    run_at: None,
                    payload: Some(b"ignored-no-op".to_vec()),
                }),
            )?,
        ],
        v::copy_policy(&policy),
    )?;
    assert_eq!(
        batch
            .inputs
            .iter()
            .map(|input| input.source_index)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3, 4]
    );
    let frame = v::encode(&batch)?;
    f::append(&mut log, 1, frame.as_bytes())?;
    log.save_committed(f::at(1))?;
    drop((log, state, batch));
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    let (log, mut state) = recover_committed(log, state, scope.group(), scope.recovery(), limits)?;
    let outcome = v::outcome(&mut state, b"holes")?;
    assert_eq!(outcome.response(), [2, 0, 0, 0, 5, 0, 1, 0, 1, 0, 0, 0]);
    assert_eq!(outcome.intent().version(), 8);
    assert_eq!(outcome.response_version(), 6);
    for (id, event, incarnation, version) in [
        (b"i".as_slice(), None, 1_u128 << 32, 2),
        (b"i", Some("100-1"), 1_u128 << 32, 1),
        (b"i", Some("100-2"), 1_u128 << 32, 2),
        (b"j", None, (1_u128 << 32) + 2, 1),
        (b"j", Some("100-1"), (1_u128 << 32) + 2, 1),
    ] {
        v::binding(
            &state,
            id,
            event,
            (incarnation, [Some(version), None, None], None),
            &policy,
        )?;
    }
    let a = v::reference(b"i", OriginalValueRefKind::Payload, 1)?;
    let b = v::reference(b"i", OriginalValueRefKind::Payload, 2)?;
    let c = v::reference(b"j", OriginalValueRefKind::Payload, 1)?;
    let read = v::read(&state, &[&a, &b, &c, &a])?;
    drop((log, state, frame, a, b, c, policy));
    v::assert_read(
        &read,
        &[b"A", b"B", b"C"],
        &[Some(0), Some(1), Some(2), Some(0)],
    );
    let cohort = v::cohort(&dir)?;
    assert_value_families(&cohort[1]);
    drop(f::open_pair(&dir)?);
    assert_eq!(v::cohort(&dir)?, cohort);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn assert_value_families(rows: &f::Rows) {
    for (family, count) in [(8, 3), (9, 3), (10, 2), (11, 3)] {
        assert_eq!(
            rows.keys()
                .filter(|key| key.get(6) == Some(&family))
                .count(),
            count
        );
    }
}
