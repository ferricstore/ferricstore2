use super::{fixtures as f, *};

#[test]
fn durable_step_limits_and_unupgraded_profile_publish_nothing() -> TestResult {
    let directory = common::directory("step-admission")?;
    let path = directory.join("state");
    let mut store = f::fresh(&path)?;
    f::seed(&mut store)?;
    drop(store);
    let rows = common::raw_rows(&path)?;
    let manifest = values::raw_manifest(&path)?;
    let frame = f::frame(2, Some(b"new"), b"w:0:1")?;
    for case in 0..7 {
        let mut cap = f::limits();
        match case {
            0 => cap.maximum_value_rows = 0,
            1 => cap.maximum_value_bytes = 1,
            2 => cap.maximum_value_work = 1,
            3 => cap.maximum_value_temporary_bytes = 1,
            4 => cap.record_batch.maximum_read_steps = 1,
            5 => cap.record_batch.maximum_adapter_bytes = 1,
            _ => cap.record_batch.workspace_bytes = 1,
        }
        let mut current = common::open(&path)?;
        assert!(
            apply_step_continue_transaction(
                &mut current,
                &common::scope()?,
                frame.as_bytes(),
                common::log(1),
                cap
            )
            .is_err(),
            "case {case}"
        );
        drop(current);
        assert_eq!(common::raw_rows(&path)?, rows);
        assert_eq!(values::raw_manifest(&path)?, manifest);
    }
    let legacy = directory.join("unupgraded");
    let mut legacy_store = values::fresh(&legacy)??;
    f::seed(&mut legacy_store)?;
    drop(legacy_store);
    let old = common::raw_rows(&legacy)?;
    let mut legacy_store = common::open(&legacy)?;
    assert!(matches!(
        apply_step_continue_transaction(
            &mut legacy_store,
            &common::scope()?,
            frame.as_bytes(),
            common::log(1),
            f::limits()
        ),
        Err(WorkflowValueTransactionError::UnsupportedProfile)
    ));
    drop(legacy_store);
    assert_eq!(common::raw_rows(&legacy)?, old);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
