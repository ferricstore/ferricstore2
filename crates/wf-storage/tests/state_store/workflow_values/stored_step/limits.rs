use super::{fixtures as s, *};
use wf_storage::transaction::apply_step_continue_transaction;

#[test]
fn stored_step_caps_profiles_and_omission_preserve_actual_cohorts() -> TestResult {
    let dir = f::directory("stored-step-caps")?;
    let path = dir.join("state");
    let mut store = c::fresh(&path, true)?;
    s::seed(&mut store)?;
    let frame = s::frame(None)?;
    assert_eq!(
        s::apply(&mut store, &frame, 2)?.status(),
        RetainedStatus::Accepted
    );
    s::assert_map(&store, 1, b"queued")?;
    drop(store);
    let before = c::cohort(&path)?;
    for field in 0..4 {
        let mut limits = v::limits();
        match field {
            0 => limits.maximum_value_rows = 0,
            1 => limits.maximum_value_bytes = 0,
            2 => limits.maximum_value_work = 0,
            _ => limits.maximum_value_temporary_bytes = 0,
        }
        let mut store = f::open(&path)?;
        assert!(
            apply_stored_step_continue_transaction(
                &mut store,
                &f::scope()?,
                frame.as_bytes(),
                f::log(3),
                limits
            )
            .is_err()
        );
        drop(store);
        assert_eq!(c::cohort(&path)?, before);
    }
    let mut store = f::open(&path)?;
    assert!(
        apply_step_continue_transaction(
            &mut store,
            &f::scope()?,
            frame.as_bytes(),
            f::log(3),
            v::limits()
        )
        .is_err()
    );
    drop(store);
    assert_eq!(c::cohort(&path)?, before);
    let alias1 = dir.join("alias1");
    let mut store = c::fresh(&alias1, false)?;
    c::create(&mut store, b"i", b"queued", (0, 1000, 1000), None)?;
    let claim = c::phase(b"claim", 1000, b"queued", b"contract-step-worker", 1)?;
    c::apply(&mut store, &claim, 1)?;
    assert_eq!(
        apply_stored_step_continue_transaction(
            &mut store,
            &f::scope()?,
            frame.as_bytes(),
            f::log(2),
            v::limits()
        )
        .err(),
        Some(WorkflowValueTransactionError::UnsupportedProfile)
    );
    drop(store);
    refuse_layout7(&dir, &frame)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn refuse_layout7(dir: &std::path::Path, frame: &PreparedBytes) -> TestResult {
    use wf_storage::state::{StateStoreGenesis, StateStoreLimits};
    let scope = f::scope()?;
    let path = dir.join("layout7");
    let placements = f::placements()?;
    let mut store = RedbStateStore::create_stored_value_database(
        redb::Builder::new().create(&path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0)?,
            placements: &placements,
            policies: &[],
        },
        v::defaults(),
        StateStoreLimits::default(),
    )?;
    assert_eq!(
        apply_stored_step_continue_transaction(
            &mut store,
            &scope,
            frame.as_bytes(),
            f::log(0),
            v::limits()
        )
        .err(),
        Some(WorkflowValueTransactionError::UnsupportedProfile)
    );
    drop(store);
    assert!(v::raw_manifest(&path)?.starts_with(b"FWSS\0\x07"));
    Ok(())
}

#[test]
fn stored_step_exact_presence_and_plain_wrapper_binary_remain_distinct() -> TestResult {
    let dir = f::directory("stored-step-presence")?;
    let typed = s::map(b"charge_card")?;
    for (index, payload) in [b"".as_slice(), b"FSV2\x02bad-etf", typed.as_slice()]
        .into_iter()
        .enumerate()
    {
        let path = dir.join(format!("stored-{index}"));
        let mut store = c::fresh(&path, true)?;
        s::seed(&mut store)?;
        let frame = s::frame(Some(payload))?;
        assert_eq!(
            s::apply(&mut store, &frame, 2)?.status(),
            RetainedStatus::Accepted
        );
        assert_eq!(s::read_value(&store, 3)?, payload);
        drop(store);
        let reopened = f::open(&path)?;
        assert_eq!(s::read_value(&reopened, 3)?, payload);
        drop(reopened);
    }
    let mut store = c::fresh(&dir.join("plain"), true)?;
    s::seed(&mut store)?;
    let plain = wf_ports::log::encode_step_continue(
        s::request(Some(&typed))?,
        v::limits().record_batch.codec,
        v::limits().record_batch.workspace_bytes,
    )?;
    assert_eq!(
        c::retained_record(apply_step_continue_transaction(
            &mut store,
            &f::scope()?,
            plain.as_bytes(),
            f::log(2),
            v::limits()
        )?)?
        .status(),
        RetainedStatus::Accepted
    );
    let mut wrapped = b"FSV2\x01".to_vec();
    wrapped.extend_from_slice(&typed);
    assert_eq!(s::read_value(&store, 3)?, wrapped);
    drop(store);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
