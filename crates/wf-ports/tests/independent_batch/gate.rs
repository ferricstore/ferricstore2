use super::*;
use wf_domain::workflow::request::RetainedMetadata;
use wf_ports::log::{
    independent_record_batch_response_matches, record_batch_retained_status,
    record_batch_retained_summary,
};

fn at(
    now: u64,
    mut items: Vec<PreparedBatchItem>,
    codec: CodecLimits,
) -> TestResult<wf_ports::log::PreparedBytes> {
    for item in &mut items {
        item.command.now = TimestampMillis::new(now);
    }
    let mut request = request(items)?;
    request.stamp = clock(now)?;
    Ok(encode_independent_record_batch(request, codec, WORKSPACE)?)
}

fn inspect<'input, 'retained>(
    bytes: &'input [u8],
    control: ControlState,
    original: Option<&'retained RetainedOutcome>,
    index: u64,
    codec: CodecLimits,
) -> TestResult<RecordBatchInspection<'input, 'retained>> {
    let scope = scope()?;
    Ok(inspect_independent_record_batch_point(
        prepare_independent_record_batch(bytes, codec, WORKSPACE)?,
        log(index),
        PointGateView {
            scope: &scope,
            control,
            outcome: original,
            retained_nonempty: original.is_some(),
        },
        codec,
        WORKSPACE,
    )?)
}

fn pair() -> TestResult<Vec<PreparedBatchItem>> {
    Ok(vec![item(b"i", retry()?)?, item(b"i", cancel(1))?])
}

fn altered_response(original: &RetainedOutcome, response: Vec<u8>) -> TestResult<RetainedOutcome> {
    Ok(RetainedOutcome::new(
        RetainedMetadata {
            scope: original.scope().clone(),
            request: original.request().clone(),
            intent: original.intent(),
            original_position: original.original_position(),
            original_time: original.original_time(),
            expires_at: original.expires_at(),
            status: original.status(),
        },
        1,
        response,
        limits().admission(),
    )?)
}

#[test]
fn counted_response_binds_plan_and_replay() -> TestResult {
    assert_closed_rows()?;
    let records = [claimed(b"i")?];
    let bytes = at(2, pair()?, limits())?;
    let plan = decide(&records, &[b"i"], bytes.as_bytes(), limits(), WORKSPACE, 0)?;
    let RecordBatchApplyDecision::IndependentFresh { operation, .. } = plan.decision() else {
        return Err("not independent".into());
    };
    assert_eq!(outcome(&plan)?.response(), [2, 0, 0, 0, 2, 0, 0]);
    assert_eq!(
        record_batch_retained_summary(outcome(&plan)?.intent(), 1, outcome(&plan)?.response())?,
        (RetainedStatus::Accepted, Some(2))
    );
    assert!(independent_record_batch_response_matches(
        operation,
        &[2, 0, 0, 0, 2, 0, 0]
    )?);
    assert!(!independent_record_batch_response_matches(
        operation,
        &[2, 0, 0, 0, 2, 0, 1, 0, 4]
    )?);
    assert!(!independent_record_batch_response_matches(
        operation,
        &[2, 0, 0, 0, 1, 0]
    )?);
    assert!(
        independent_record_batch_response_matches(operation, &[2, 0, 0, 0, 2, 0, 0, 0]).is_err()
    );
    let control = plan.control().next();
    let (_, decision) = plan.into_parts();
    let RecordBatchApplyDecision::IndependentFresh {
        operation,
        outcome: original,
    } = decision
    else {
        return Err("not independent".into());
    };
    drop(operation);
    drop(records);
    assert_replay(&original, control)?;
    assert_count_precedence(&original, control, bytes.as_bytes())?;
    assert_status_precedence(&original, control, bytes.as_bytes())?;
    assert_whole_rejection_replay()?;
    Ok(())
}

fn assert_closed_rows() -> TestResult {
    let intent = ClientIntent::independent_record_batch([0x11; 32]);
    for row in [&[0][..], &[1, 0, 0], &[1, 0, 8, 0], &[1, 4, 0], &[1, 5, 0]] {
        let body = [vec![2, 0, 0, 0, 1], row.to_vec()].concat();
        assert_eq!(
            record_batch_retained_summary(intent, 1, &body)?,
            (RetainedStatus::Accepted, Some(1))
        );
    }
    for row in [
        &[1, 1, 4][..],
        &[1, 2, 0],
        &[1, 3, 0],
        &[1, 6, 0],
        &[1, 0, 8, 255],
        &[1, 0, 255],
        &[1, 5, 1],
        &[2],
    ] {
        let body = [vec![2, 0, 0, 0, 1], row.to_vec()].concat();
        assert!(record_batch_retained_summary(intent, 1, &body).is_err());
    }
    for body in [
        &[][..],
        &[0],
        &[2],
        &[2, 0, 0, 0, 0],
        &[2, 0, 1, 134, 161],
        &[2, 0, 0, 0, 1],
        &[2, 0, 0, 0, 1, 1, 0, 8],
        &[2, 0, 0, 0, 1, 0, 0],
        &[3],
    ] {
        assert!(record_batch_retained_summary(intent, 1, body).is_err());
    }
    assert_eq!(
        record_batch_retained_summary(intent, 1, &[1, 1, 4])?,
        (
            RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Admission(
                AdmissionError::DecisionBytes
            ))),
            None,
        )
    );
    assert_eq!(
        record_batch_retained_status(ClientIntent::record_batch([0x11; 32]), 1, &[0])?,
        RetainedStatus::Accepted
    );
    assert!(
        record_batch_retained_status(
            ClientIntent::record_batch([0x11; 32]),
            1,
            &[2, 0, 0, 0, 1, 0]
        )
        .is_err()
    );
    assert!(matches!(
        record_batch_retained_summary(intent, 2, &[2, 0, 0, 0, 1, 0]),
        Err(CodecError::UnsupportedVersion)
    ));
    Ok(())
}

fn assert_replay(original: &RetainedOutcome, control: ControlState) -> TestResult {
    let bytes = at(9, pair()?, limits())?;
    let complete = inspect(bytes.as_bytes(), control, Some(original), 1, limits())?;
    let RecordBatchInspection::Complete(plan) = complete else {
        return Err("replayed as fresh".into());
    };
    let RecordBatchApplyDecision::Replay(replayed) = plan.decision() else {
        return Err("not replay".into());
    };
    assert_eq!(std::ptr::from_ref(*replayed), std::ptr::from_ref(original));
    assert_eq!(replayed.response(), [2, 0, 0, 0, 2, 0, 0]);
    assert_eq!(replayed.original_position(), log(0));
    assert_eq!(replayed.original_time(), TimestampMillis::new(2));
    assert_eq!(
        plan.control()
            .next()
            .last_entry()
            .map(wf_domain::workflow::control::EntryIdentity::log),
        Some(log(1))
    );
    assert_eq!(plan.control().execution_time(), TimestampMillis::new(9));
    Ok(())
}

fn assert_count_precedence(
    original: &RetainedOutcome,
    control: ControlState,
    initial: &[u8],
) -> TestResult {
    let wrong_count = altered_response(original, vec![2, 0, 0, 0, 1, 0])?;
    let replay = at(9, pair()?, limits())?;
    assert!(inspect(replay.as_bytes(), control, Some(&wrong_count), 1, limits()).is_err());
    let exact = inspect(initial, control, Some(&wrong_count), 0, limits())?;
    assert!(
        matches!(exact, RecordBatchInspection::Complete(plan) if matches!(plan.decision(), RecordBatchApplyDecision::LogReplay))
    );
    let conflict = at(
        9,
        vec![item(b"i", retry()?)?, item(b"i", cancel(2))?],
        limits(),
    )?;
    assert!(
        matches!(inspect(conflict.as_bytes(), control, Some(&wrong_count), 1, limits())?,
        RecordBatchInspection::Complete(plan) if matches!(plan.decision(), RecordBatchApplyDecision::IntentConflict))
    );
    let expired = at(2_000, pair()?, limits())?;
    assert!(
        matches!(inspect(expired.as_bytes(), control, Some(&wrong_count), 1, limits())?,
        RecordBatchInspection::Complete(plan) if matches!(plan.decision(), RecordBatchApplyDecision::Expired))
    );
    Ok(())
}

fn assert_whole_rejection_replay() -> TestResult {
    let codec = CodecLimits::new(AdmissionLimits::new(256, 2_867, 1)?);
    let bytes = at(2, vec![item(b"c", create())?], codec)?;
    let plan = decide(&[], &[b"c"], bytes.as_bytes(), codec, WORKSPACE, 0)?;
    assert_eq!(outcome(&plan)?.response(), [1, 1, 4]);
    let control = plan.control().next();
    let (_, decision) = plan.into_parts();
    let RecordBatchApplyDecision::IndependentFresh {
        operation,
        outcome: original,
    } = decision
    else {
        return Err("not independent".into());
    };
    drop(operation);
    let replay = at(9, vec![item(b"c", create())?], codec)?;
    let inspected = inspect(replay.as_bytes(), control, Some(&original), 1, codec)?;
    let RecordBatchInspection::Complete(plan) = inspected else {
        return Err("whole rejection became fresh".into());
    };
    assert!(
        matches!(plan.decision(), RecordBatchApplyDecision::Replay(value) if value.response() == [1, 1, 4])
    );
    Ok(())
}

fn assert_status_precedence(
    original: &RetainedOutcome,
    control: ControlState,
    initial: &[u8],
) -> TestResult {
    let wrong_status = RetainedOutcome::new(
        RetainedMetadata {
            scope: original.scope().clone(),
            request: original.request().clone(),
            intent: original.intent(),
            original_position: original.original_position(),
            original_time: original.original_time(),
            expires_at: original.expires_at(),
            status: RetainedStatus::IndexedRejected(IndexRejection::Batch(
                BatchRejection::Admission(AdmissionError::DecisionBytes),
            )),
        },
        1,
        original.response().to_vec(),
        limits().admission(),
    )?;
    assert_eq!(
        record_batch_retained_summary(wrong_status.intent(), 1, wrong_status.response())?,
        (RetainedStatus::Accepted, Some(2))
    );
    let replay = at(9, pair()?, limits())?;
    assert!(inspect(replay.as_bytes(), control, Some(&wrong_status), 1, limits()).is_err());
    let exact = inspect(initial, control, Some(&wrong_status), 0, limits())?;
    assert!(
        matches!(exact, RecordBatchInspection::Complete(plan) if matches!(plan.decision(), RecordBatchApplyDecision::LogReplay))
    );
    let conflict = at(
        9,
        vec![item(b"i", retry()?)?, item(b"i", cancel(2))?],
        limits(),
    )?;
    assert!(
        matches!(inspect(conflict.as_bytes(), control, Some(&wrong_status), 1, limits())?,
        RecordBatchInspection::Complete(plan) if matches!(plan.decision(), RecordBatchApplyDecision::IntentConflict))
    );
    let expired = at(2_000, pair()?, limits())?;
    assert!(
        matches!(inspect(expired.as_bytes(), control, Some(&wrong_status), 1, limits())?,
        RecordBatchInspection::Complete(plan) if matches!(plan.decision(), RecordBatchApplyDecision::Expired))
    );
    Ok(())
}
