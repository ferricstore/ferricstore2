use super::*;

fn tuple_samples() {
    // Literal builtins executed on OTP28 AND OTP29 before the Rust helper exists.
    for (attempt, now, expected) in [
        (0, 0, 96_899_277),
        (1, 1, 25_882_645),
        (1, 2_000, 108_614_096),
        (134_217_727, 2_000, 50_680_942),
        (134_217_728, 2_000, 119_561_258),
        (134_217_729, 2_000, 100_361_419),
        (1, 134_217_727, 92_640_007),
        (1, 134_217_728, 70_890_341),
        (1, 134_217_729, 47_656_064),
        (576_460_752_303_423_487, 576_460_752_303_423_488, 1_221_676),
        (
            1_152_921_504_606_846_976,
            1_152_921_504_606_846_977,
            114_920_685,
        ),
        (
            9_223_372_036_854_775_807,
            9_223_372_036_854_775_808,
            764_060,
        ),
        (u64::MAX, u64::MAX, 127_598_732),
    ] {
        assert_eq!(source_retry_jitter(b"i", attempt, now).get(), expected);
    }
    for (id, attempt, now, expected) in [
        (vec![0], 1, 2_000, 39_223_399),
        (vec![0, 255, 0, 128], 3, 1_700_000_000_000, 34_316_261),
        (vec![1; 11], 17, 4_294_967_295, 31_365_439),
        (vec![1; 12], 17, 4_294_967_296, 16_193_658),
        (vec![1; 13], 17, 4_294_967_297, 57_661_632),
        (
            [0, 255, 17].repeat(21_845),
            4_294_967_295,
            u64::MAX,
            62_559_175,
        ),
    ] {
        assert_eq!(source_retry_jitter(&id, attempt, now).get(), expected);
    }
}

fn command(token: &[u8], run_at: Option<u64>) -> TestResult<CommittedCommand> {
    Ok(CommittedCommand {
        id: RecordId::new(b"i".to_vec())?,
        now: TimestampMillis::new(2_000),
        operation: Operation::RetryWithPolicy(PolicyRetry {
            guard: LeaseGuard {
                token: LeaseToken::new(token.to_vec())?,
                fence: Fence::new(1),
            },
            run_at: run_at.map(TimestampMillis::new),
            policy_override: RetryOverride::default(),
            jitter: JitterSample::new(0),
            payload: None,
            error: None,
        }),
    })
}

fn result<'plan>(
    decision: &'plan IndependentIndexedDecision<'_>,
) -> TestResult<&'plan wf_domain::workflow::batch::ScopedPlan> {
    let IndependentIndexedDecision::Accepted(plan) = decision else {
        return Err("whole refusal".into());
    };
    plan.results()[0]
        .as_ref()
        .map_err(|_| "item refused".into())
}

fn authoritative_timing_and_errors() -> TestResult {
    let running = claimed()?;
    let retry = command(b"l", None)?;
    let decision = decide(&running, &retry, 1)??;
    let write = result(&decision)?
        .plan()
        .write()
        .ok_or("source write absent")?;
    // Default1000, spread200, literal sample108614096%401-200=-162.
    assert_eq!(
        write.record.next_run_at(),
        Some(TimestampMillis::new(2_838))
    );
    assert_eq!(write.record.attempts(), 1);
    assert_eq!(write.record.version(), 3);
    assert_eq!(write.record.state(), b"q");
    assert!(write.record.lease().is_none());
    // Private explicit-sample behavior remains exact and demonstrably different.
    let private = reduce(Some(&running), &retry)?;
    assert_eq!(
        private.write().ok_or("private write")?.record.next_run_at(),
        Some(TimestampMillis::new(2_800))
    );
    let explicit = command(b"l", Some(9_000))?;
    let decision = decide(&running, &explicit, 1)??;
    assert_eq!(
        result(&decision)?
            .plan()
            .write()
            .ok_or("explicit write")?
            .record
            .next_run_at(),
        Some(TimestampMillis::new(9_000))
    );
    let exhausted = with_attempts(&running, 3)?;
    let decision = decide(&exhausted, &explicit, 1)??;
    let record = &result(&decision)?
        .plan()
        .write()
        .ok_or("exhausted write")?
        .record;
    assert_eq!(record.state(), b"failed");
    assert_eq!(record.next_run_at(), None);
    assert_eq!(record.attempts(), 4);
    let overflow = with_attempts(&running, u64::MAX)?;
    for (token, expected) in [
        (b"wrong".as_slice(), DomainError::StaleLease),
        (
            b"l".as_slice(),
            DomainError::CounterOverflow(wf_domain::value::Counter::Attempts),
        ),
    ] {
        let retry = command(token, None)?;
        let decision = decide(&overflow, &retry, 1)??;
        let IndependentIndexedDecision::Accepted(plan) = decision else {
            return Err("unexpected whole refusal".into());
        };
        assert_eq!(
            plan.results()[0],
            Err(IndexRejection::Batch(BatchRejection::Domain(expected)))
        );
        assert_eq!(plan.history_plans().count(), 0);
        assert_eq!(plan.final_indices().count(), 0);
    }
    assert!(matches!(
        decide(&running, &retry, 0)?,
        Err(ApplyError::InvalidAuthority)
    ));
    assert!(matches!(
        decide(&running, &retry, 2)?,
        Err(ApplyError::InvalidAuthority)
    ));
    assert_eq!(running.version(), 2);
    assert_eq!(running.attempts(), 0);
    assert_eq!(overflow.attempts(), u64::MAX);
    Ok(())
}

#[test]
fn public_retry_tuple_samples_and_reached_error_order() -> TestResult {
    tuple_samples();
    authoritative_timing_and_errors()
}
