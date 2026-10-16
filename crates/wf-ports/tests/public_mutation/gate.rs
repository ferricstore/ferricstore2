use super::*;

fn scalar_grammar(intent: ClientIntent) -> TestResult {
    assert_eq!(
        public_mutation_retained_status(intent, 2, &[0])?,
        RetainedStatus::Accepted
    );
    let missing = RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
        DomainError::NotFound,
    )));
    assert_eq!(
        public_mutation_retained_status(intent, 2, &[2, 0, 0])?,
        missing
    );
    for body in [
        &[][..],
        &[0, 0],
        &[1, 0, 0],
        &[2, 1, 4],
        &[2, 2, 0],
        &[2, 3, 0],
        &[2, 6, 0],
        &[2, 0, 0, 0],
        &[2, 0, 255],
        &[3],
    ] {
        assert!(public_mutation_retained_status(intent, 2, body).is_err());
    }
    assert!(public_mutation_retained_status(intent, 1, &[0]).is_err());
    assert!(public_mutation_retained_status(ClientIntent::record_batch([0; 32]), 2, &[0]).is_err());
    assert!(
        public_mutation_retained_status(ClientIntent::independent_record_batch([0; 32]), 2, &[0])
            .is_err()
    );
    Ok(())
}

#[test]
fn public_item_profile_binds_evidence_effects_and_gate() -> TestResult {
    let (bytes, intent) = encoded(2_000, &fields())?;
    scalar_grammar(intent)?;
    let plan = decide_missing(bytes.as_bytes(), WORKSPACE)?;
    let original = retained(&plan)?;
    assert_eq!(original.intent(), intent);
    assert_eq!(original.response_version(), 2);
    assert_eq!(original.response(), [2, 0, 0]);
    assert_eq!(
        original.status(),
        RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
            DomainError::NotFound
        )))
    );
    let RecordBatchApplyDecision::IndependentFresh { operation, .. } = plan.decision() else {
        return Err("not public decision".into());
    };
    assert!(
        matches!(operation, IndependentIndexedDecision::Accepted(value) if value.results().len()==1)
    );
    assert!(public_mutation_response_matches(operation, &[2, 0, 0])?);
    assert!(!public_mutation_response_matches(operation, &[0])?);
    assert!(public_mutation_response_matches(operation, &[1, 0, 0]).is_err());
    let control = plan.control().next();
    let (retry, retry_intent) = encoded(2_100, &fields())?;
    assert_eq!(retry_intent, intent);
    let RecordBatchInspection::Complete(replayed) =
        inspect(retry.as_bytes(), control, Some(original), 1)?
    else {
        return Err("not Replay complete".into());
    };
    assert!(
        matches!(replayed.decision(), RecordBatchApplyDecision::Replay(value) if std::ptr::eq(*value,original))
    );
    assert_eq!(
        replayed.control().next().position(),
        AppliedPosition::new(1)
    );
    let wrong_status = altered(
        original,
        RetainedStatus::Accepted,
        original.response().to_vec(),
    )?;
    assert!(inspect(retry.as_bytes(), control, Some(&wrong_status), 1).is_err());
    let malformed = altered(original, original.status(), vec![2, 0, 0, 0])?;
    assert!(inspect(retry.as_bytes(), control, Some(&malformed), 1).is_err());
    assert_bypasses(bytes.as_bytes(), control, &malformed)?;
    Ok(())
}

fn assert_bypasses(
    original: &[u8],
    control: ControlState,
    malformed: &RetainedOutcome,
) -> TestResult {
    let RecordBatchInspection::Complete(exact) = inspect(original, control, Some(malformed), 0)?
    else {
        return Err("not exact log".into());
    };
    assert!(matches!(
        exact.decision(),
        RecordBatchApplyDecision::LogReplay
    ));
    let fields = [
        ("lease_token", MutationFieldValue::Bytes(b"l")),
        ("fencing_token", MutationFieldValue::Integer(2)),
    ];
    let (changed, _) = encoded(2_100, &fields)?;
    let RecordBatchInspection::Complete(conflict) =
        inspect(changed.as_bytes(), control, Some(malformed), 1)?
    else {
        return Err("not conflict".into());
    };
    assert!(matches!(
        conflict.decision(),
        RecordBatchApplyDecision::IntentConflict
    ));
    let (expired, _) = encoded(3_000, &self::fields())?;
    let RecordBatchInspection::Complete(expired) =
        inspect(expired.as_bytes(), control, Some(malformed), 1)?
    else {
        return Err("not expired".into());
    };
    assert!(matches!(
        expired.decision(),
        RecordBatchApplyDecision::Expired
    ));
    let mut forged = original.to_vec();
    forged[29] ^= 1;
    assert!(matches!(
        prepare_public_mutation(&forged, limits(), WORKSPACE),
        Err(RecordBatchApplyError::IntentMismatch)
    ));
    let mut time = original.to_vec();
    time[100] ^= 1;
    assert!(prepare_public_mutation(&time, limits(), WORKSPACE).is_err());
    Ok(())
}
