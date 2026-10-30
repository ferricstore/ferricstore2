use super::{fixtures as f, views as v, *};
use std::mem::size_of;

fn check_effect_caps(decoded: &DecodedRequest<'_>, plan: &RecordBatchApplyPlan<'_>) {
    assert!(matches!(
        inspect_record_batch_value_effects(
            decoded,
            plan,
            size_of::<RecordBatchValueEffect>(),
            WORKSPACE
        ),
        Ok(Some(_))
    ));
    assert!(matches!(
        inspect_record_batch_value_effects(
            decoded,
            plan,
            size_of::<RecordBatchValueEffect>() - 1,
            WORKSPACE
        ),
        Err(RecordBatchValueEffectsError::Limit)
    ));
    assert!(matches!(
        inspect_record_batch_value_effects(decoded, plan, size_of::<RecordBatchValueEffect>(), 0),
        Err(RecordBatchValueEffectsError::Limit)
    ));
}

#[test]
fn step_workspaces_effect_bounds_and_foreign_drivers_refuse() -> TestResult {
    let frame = f::frame(2, b"charge", Some(b"\0\xff"))?;
    drop(f::fresh(frame.as_bytes(), WORKSPACE)?);
    let record = f::record()?;
    let exact = 4 * frame.as_bytes().len()
        + 2 * f::limits()?.admission().decision_bytes()
        + 3072
        + 192
        + 256
        + size_of::<DecodedRequest<'static>>()
        + size_of::<Option<RecordBatchValueEffects>>()
        + size_of::<RecordBatchValueEffect>();
    v::with_view(&record, StateMode::Parallel, |view| {
        let plan =
            decide_fresh_step_continue_touched(f::fresh(frame.as_bytes(), exact)?, view, 0, 100)?;
        let decoded = prepare_step_continue(frame.as_bytes(), f::limits()?, WORKSPACE)?;
        check_effect_caps(&decoded, &plan);
        Ok(())
    })?;
    v::with_view(&record, StateMode::Parallel, |view| {
        let result = decide_fresh_step_continue_touched(
            f::fresh(frame.as_bytes(), exact - 1)?,
            view,
            0,
            100,
        );
        assert!(
            matches!(result,Err(RecordBatchApplyError::Workspace { attempted, maximum }) if attempted==exact && maximum==exact-1)
        );
        Ok(())
    })?;
    v::with_view(&record, StateMode::Parallel, |view| {
        assert!(
            decide_fresh_step_continue_touched(
                f::fresh(frame.as_bytes(), exact + 17)?,
                view,
                17,
                100
            )
            .is_ok()
        );
        Ok(())
    })?;
    v::with_view(&record, StateMode::Parallel, |view| {
        assert!(matches!(
            decide_fresh_step_continue_touched(
                f::fresh(frame.as_bytes(), exact + 17)?,
                view,
                18,
                100
            ),
            Err(RecordBatchApplyError::Workspace { .. })
        ));
        Ok(())
    })?;
    v::with_view(&record, StateMode::Parallel, |view| {
        assert!(matches!(
            decide_fresh_atomic_start_touched(f::fresh(frame.as_bytes(), WORKSPACE)?, view, 0, 100),
            Err(RecordBatchApplyError::Encoding(
                CodecError::UnsupportedVersion
            ))
        ));
        Ok(())
    })?;
    v::with_view(&record, StateMode::Parallel, |view| {
        assert!(matches!(
            decide_fresh_record_batch_touched(f::fresh(frame.as_bytes(), WORKSPACE)?, view, 0, 100),
            Err(RecordBatchApplyError::Encoding(
                CodecError::UnsupportedVersion
            ))
        ));
        Ok(())
    })?;
    v::with_view(&record, StateMode::Parallel, |view| {
        assert!(matches!(
            decide_fresh_step_continue_touched(f::fresh(frame.as_bytes(), WORKSPACE)?, view, 0, 0),
            Err(RecordBatchApplyError::Retained(
                RequestValueError::InvalidExpiry
            ))
        ));
        Ok(())
    })?;
    Ok(())
}
