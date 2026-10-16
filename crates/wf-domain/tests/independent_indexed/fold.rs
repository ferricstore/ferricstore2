use super::*;

#[test]
fn independent_indexed_retry_cancel_preserves_history_and_original_final() -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = claimed(b"i")?;
    let policy = policy(&[(b"queued", StateMode::Fifo)])?;
    let commands = [retry(&record, None)?, cancel(b"i", 1)?];
    for mode in [None, Some(policy.policy())] {
        let plan = accepted(decide(
            &[entry(&partition, &record, 0)],
            &[(&partition, &commands[0], 0), (&partition, &commands[1], 0)],
            mode,
            None,
            AdmissionLimits::new(256, 5_504, 2)?,
        )?)?;
        assert_eq!(plan.results().len(), 2);
        let first = write(success(plan.results(), 0)?)?;
        let last = write(success(plan.results(), 1)?)?;
        assert_eq!(first.expected_version, Some(2));
        assert_eq!(first.record.version(), 3);
        assert_eq!(first.record.state(), b"queued");
        assert_eq!(first.record.attempts(), 1);
        assert_eq!(
            first.record.next_run_at(),
            Some(TimestampMillis::new(3_000))
        );
        assert_eq!(last.expected_version, Some(3));
        assert_eq!(last.record.version(), 4);
        assert_eq!(last.record.state(), b"cancelled");
        assert_eq!(plan.final_indices().collect::<Vec<_>>(), [1]);
        let original = plan.originals().next().flatten().ok_or("original absent")?;
        assert!(std::ptr::eq(original.record, &raw const record));
        assert_eq!(original.record.version(), 2);
        assert_eq!(
            plan.metadata()
                .next()
                .ok_or("metadata absent")?
                .state_enter_seq,
            99
        );
        assert_eq!(plan.consumed_ordinals(), 0);
        assert_eq!(plan.next_ordinal(), Some(5));
        let events: Vec<_> = plan
            .history_plans()
            .map(|p| p.plan().history().ok_or("history absent"))
            .collect::<Result<_, _>>()?;
        assert_eq!(
            events
                .iter()
                .map(|e| (e.kind, e.version, e.event_id.as_str()))
                .collect::<Vec<_>>(),
            [
                (EventKind::Retry, 3, "2000-3"),
                (EventKind::Cancelled, 4, "2000-4")
            ]
        );
        let deltas = plan.deltas().collect::<Result<Vec<_>, _>>()?;
        assert_eq!(deltas.len(), 1);
        assert!(deltas[0].after.is_none());
        match (mode, deltas[0].before) {
            (None, Some(IndexRows::Parallel { due })) => {
                assert_eq!(due.due, TimestampMillis::new(31_000));
            }
            (Some(_), Some(IndexRows::Fifo { member, .. })) => assert_eq!(member.score(), -1),
            _ => return Err("original index family lost".into()),
        }
    }
    assert_eq!(record.state(), b"running");
    assert_eq!(record.version(), 2);
    Ok(())
}

#[test]
fn independent_indexed_rejection_does_not_advance_latest_or_ordinals() -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let policy = policy(&[(b"q", StateMode::Fifo), (b"z", StateMode::Parallel)])?;
    let commands = [
        create(b"i", b"q")?,
        create(b"i", b"q")?,
        transition(b"i", b"q", b"z")?,
        cancel(b"i", 0)?,
    ];
    let refs = [
        (&partition, &commands[0], 1),
        (&partition, &commands[1], 0),
        (&partition, &commands[2], 0),
        (&partition, &commands[3], 0),
    ];
    let plan = accepted(decide(
        &[],
        &refs,
        Some(policy.policy()),
        None,
        AdmissionLimits::new(256, 10_848, 4)?,
    )?)?;
    assert_eq!(plan.results().len(), 4);
    assert_eq!(plan.results()[0], Err(IndexRejection::FifoPriority));
    assert_eq!(write(success(plan.results(), 1)?)?.expected_version, None);
    assert_eq!(
        write(success(plan.results(), 2)?)?.expected_version,
        Some(1)
    );
    let last = write(success(plan.results(), 3)?)?;
    assert_eq!(last.expected_version, Some(2));
    assert_eq!(last.record.version(), 3);
    assert_eq!(last.record.state(), b"cancelled");
    assert_eq!(plan.final_indices().collect::<Vec<_>>(), [3]);
    assert_eq!(plan.originals().collect::<Vec<_>>(), [None]);
    assert_eq!(
        plan.metadata()
            .next()
            .ok_or("metadata absent")?
            .state_enter_seq,
        (7_u128 << 32) + 6
    );
    assert_eq!(plan.consumed_ordinals(), 2);
    assert_eq!(plan.next_ordinal(), Some(7));
    assert_eq!(
        plan.history_plans()
            .map(|p| p.plan().history().map(|h| (h.kind, h.version)))
            .collect::<Vec<_>>(),
        [
            Some((EventKind::Created, 1)),
            Some((EventKind::Transitioned, 2)),
            Some((EventKind::Cancelled, 3))
        ]
    );
    let deltas = plan.deltas().collect::<Result<Vec<_>, _>>()?;
    assert!(deltas[0].before.is_none() && deltas[0].after.is_none());
    assert_failed_transition_keeps_predecessor()?;
    Ok(())
}

fn assert_failed_transition_keeps_predecessor() -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = initial(b"i", b"q")?;
    let policy = policy(&[(b"bad", StateMode::Fifo)])?;
    let commands = [
        transition(b"i", b"q", b"bad")?,
        transition(b"i", b"q", b"z")?,
    ];
    let plan = accepted(decide(
        &[entry(&partition, &record, 2)],
        &[(&partition, &commands[0], 0), (&partition, &commands[1], 0)],
        Some(policy.policy()),
        None,
        AdmissionLimits::default(),
    )?)?;
    assert_eq!(plan.results()[0], Err(IndexRejection::FifoPriority));
    let changed = write(success(plan.results(), 1)?)?;
    assert_eq!(changed.expected_version, Some(1));
    assert_eq!(changed.record.state(), b"z");
    assert_eq!(changed.record.version(), 2);
    assert_eq!(plan.consumed_ordinals(), 1);
    assert_eq!(
        plan.metadata()
            .next()
            .ok_or("metadata absent")?
            .state_enter_seq,
        (7_u128 << 32) + 5
    );
    assert_eq!(plan.history_plans().count(), 1);
    Ok(())
}

pub(super) fn assert_domain_error_keeps_latest() -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = claimed(b"i")?;
    let extend = command(
        b"i",
        Operation::ExtendLease(ExtendLease {
            guard: guard(&record)?,
            lease_ms: 500,
        }),
    )?;
    let mut bad_guard = guard(&record)?;
    bad_guard.token = LeaseToken::new(b"wrong-token".to_vec())?;
    let bad = command(
        b"i",
        Operation::Fail(Terminal {
            guard: bad_guard,
            payload: None,
            value: None,
        }),
    )?;
    let good = command(
        b"i",
        Operation::Fail(Terminal {
            guard: guard(&record)?,
            payload: None,
            value: None,
        }),
    )?;
    let plan = accepted(decide(
        &[entry(&partition, &record, 0)],
        &[
            (&partition, &extend, 0),
            (&partition, &bad, 0),
            (&partition, &good, 0),
        ],
        None,
        None,
        AdmissionLimits::default(),
    )?)?;
    assert_eq!(plan.results().len(), 3);
    assert_eq!(plan.results()[1], Err(rejected(DomainError::StaleLease)));
    let last = write(success(plan.results(), 2)?)?;
    assert_eq!(last.expected_version, Some(3));
    assert_eq!(last.record.version(), 4);
    assert_eq!(last.record.state(), b"failed");
    assert_eq!(plan.final_indices().collect::<Vec<_>>(), [2]);
    assert_eq!(
        plan.originals()
            .next()
            .flatten()
            .ok_or("original absent")?
            .record
            .version(),
        2
    );
    assert_eq!(plan.consumed_ordinals(), 0);
    assert_eq!(plan.next_ordinal(), Some(5));
    assert_eq!(
        plan.history_plans()
            .map(|p| p.plan().history().map(|h| (h.kind, h.version)))
            .collect::<Vec<_>>(),
        [
            Some((EventKind::LeaseExtended, 3)),
            Some((EventKind::Failed, 4))
        ]
    );
    Ok(())
}
