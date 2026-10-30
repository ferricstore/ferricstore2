use super::{fixtures as f, *};

fn inspect<'input, 'retained>(
    frame: &'input PreparedBytes,
    index: u64,
    control: ControlState,
    outcome: &'retained RetainedOutcome,
) -> TestResult<RecordBatchInspection<'input, 'retained>> {
    let scope = f::scope()?;
    Ok(inspect_step_continue_point(
        prepare_step_continue(frame.as_bytes(), f::limits()?, WORKSPACE)?,
        f::log(index),
        PointGateView {
            scope: &scope,
            control,
            retained_nonempty: true,
            outcome: Some(outcome),
        },
        f::limits()?,
        WORKSPACE,
    )?)
}

#[test]
fn step_replay_uses_original_bytes_without_current_authority() -> TestResult {
    let frame = f::frame(2, b"charge", None)?;
    let record = f::record()?;
    let plan = f::plan(&frame, &record, StateMode::Parallel)?;
    let control = plan.control().next();
    let (_, RecordBatchApplyDecision::Fresh { outcome, .. }) = plan.into_parts() else {
        return Err("original Fresh".into());
    };
    let duplicate = f::frame(3, b"charge", None)?;
    let RecordBatchInspection::Complete(replay) = inspect(&duplicate, 2, control, &outcome)? else {
        return Err("replay became Fresh".into());
    };
    let RecordBatchApplyDecision::Replay(original) = replay.decision() else {
        return Err("original Replay".into());
    };
    assert!(std::ptr::eq(*original, outcome.as_ref()));
    assert_eq!(
        (
            original.original_position(),
            original.original_time().get(),
            original.expires_at().get()
        ),
        (f::log(1), 2, 102)
    );
    assert_eq!(replay.control().next().position(), AppliedPosition::new(2));
    assert_eq!(replay.control().execution_time().get(), 3);
    let mut corrupt = outcome.response().to_vec();
    corrupt[0] ^= 1;
    let corrupt =
        RetainedOutcome::new(f::metadata(&outcome), 9, corrupt, f::limits()?.admission())?;
    let conflict = f::frame(3, b"charge", Some(b"changed"))?;
    let RecordBatchInspection::Complete(conflict) = inspect(&conflict, 2, control, &corrupt)?
    else {
        return Err("conflict became Fresh".into());
    };
    assert!(matches!(
        conflict.decision(),
        RecordBatchApplyDecision::IntentConflict
    ));
    let expired = f::frame(102, b"charge", None)?;
    let RecordBatchInspection::Complete(expired) = inspect(&expired, 2, control, &corrupt)? else {
        return Err("expired became Fresh".into());
    };
    assert!(matches!(
        expired.decision(),
        RecordBatchApplyDecision::Expired
    ));
    let RecordBatchInspection::Complete(exact) = inspect(&frame, 1, control, &corrupt)? else {
        return Err("same log became Fresh".into());
    };
    assert!(matches!(
        exact.decision(),
        RecordBatchApplyDecision::LogReplay
    ));
    Ok(())
}

#[test]
fn step_retained_profile_and_replay_corruption_refuse() -> TestResult {
    let frame = f::frame(2, b"charge", None)?;
    let record = f::record()?;
    let plan = f::plan(&frame, &record, StateMode::Parallel)?;
    let control = plan.control().next();
    let (_, RecordBatchApplyDecision::Fresh { outcome, .. }) = plan.into_parts() else {
        return Err("original Fresh".into());
    };
    let duplicate = f::frame(3, b"charge", None)?;
    for offset in [0, 6, 14, 22, 30] {
        let mut corrupt = outcome.response().to_vec();
        corrupt[offset] ^= 1;
        let corrupt =
            RetainedOutcome::new(f::metadata(&outcome), 9, corrupt, f::limits()?.admission())?;
        assert!(
            inspect(&duplicate, 2, control, &corrupt).is_err(),
            "context {offset}"
        );
    }
    let mut metadata = f::metadata(&outcome);
    metadata.status = RetainedStatus::IndexedRejected(IndexRejection::NotFifoHead);
    let crossed = RetainedOutcome::new(
        metadata,
        9,
        outcome.response().to_vec(),
        f::limits()?.admission(),
    )?;
    assert!(inspect(&duplicate, 2, control, &crossed).is_err());
    assert_eq!(
        RetainedOutcome::new(f::metadata(&outcome), 1, vec![0], f::limits()?.admission()).err(),
        Some(RequestValueError::UnsupportedVersion)
    );
    let mut metadata = f::metadata(&outcome);
    metadata.intent = ClientIntent::record_batch([0; 32]);
    assert_eq!(
        RetainedOutcome::new(
            metadata,
            9,
            outcome.response().to_vec(),
            f::limits()?.admission()
        )
        .err(),
        Some(RequestValueError::UnsupportedVersion)
    );
    let mut metadata = f::metadata(&outcome);
    metadata.status = RetainedStatus::Rejected(BatchRejection::DuplicateId);
    assert_eq!(
        RetainedOutcome::new(
            metadata,
            9,
            outcome.response().to_vec(),
            f::limits()?.admission()
        )
        .err(),
        Some(RequestValueError::UnsupportedVersion)
    );
    Ok(())
}
