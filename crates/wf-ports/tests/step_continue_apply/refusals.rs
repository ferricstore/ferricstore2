use super::{fixtures as f, *};
use wf_domain::value::DomainError;

#[test]
fn fresh_step_retains_stale_and_fifo_rejections() -> TestResult {
    let healthy = f::frame(2, b"charge", None)?;
    drop(f::fresh(healthy.as_bytes(), WORKSPACE)?);
    let record = f::record()?;
    for fifo in [false, true] {
        let mut request = f::request(2, b"charge", None)?;
        let PreparedOperation::Batch(items) = &mut request.operation else {
            return Err("batch fixture".into());
        };
        let Operation::StepContinue(step) = &mut items[0].command.operation else {
            return Err("Step fixture".into());
        };
        if !fifo {
            step.guard.token = LeaseToken::new(b"wrong".to_vec())?;
        }
        let frame = encode_step_continue(request, f::limits()?, WORKSPACE)?;
        let mode = if fifo {
            StateMode::Fifo
        } else {
            StateMode::Parallel
        };
        let expected = if fifo {
            IndexRejection::DirectRunningFifoEntry
        } else {
            IndexRejection::Batch(BatchRejection::Domain(DomainError::StaleLease))
        };
        let plan = f::plan(&frame, &record, mode)?;
        let RecordBatchApplyDecision::Fresh { operation, outcome } = plan.decision() else {
            return Err("retained rejection".into());
        };
        assert_eq!(operation, &IndexedDecision::Rejected(expected));
        assert_eq!(outcome.status(), RetainedStatus::IndexedRejected(expected));
        assert_eq!(
            step_continue_retained_summary(
                outcome.intent(),
                9,
                outcome.response(),
                f::log(1),
                TimestampMillis::new(2),
                f::limits()?.admission()
            )?,
            (outcome.status(), None)
        );
        assert!(step_continue_response_matches(
            operation,
            outcome.response(),
            f::log(1),
            TimestampMillis::new(2),
            f::limits()?.admission()
        )?);
        assert!(plan.value_effects().is_none());
        let decoded = prepare_step_continue(frame.as_bytes(), f::limits()?, WORKSPACE)?;
        assert!(matches!(
            inspect_record_batch_value_effects(&decoded, &plan, 0, WORKSPACE),
            Ok(None)
        ));
    }
    assert_eq!(record.version(), 2);
    Ok(())
}
