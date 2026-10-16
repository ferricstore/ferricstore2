use super::*;

#[test]
fn independent_indexed_eight_mutations_and_policy_observations() -> TestResult {
    assert_create_transition_cancel()?;
    let retried = owned_chain(5_504, false)?;
    assert_eq!(write(success(&retried, 0)?)?.record.attempts(), 1);
    assert_complete_duplicate()?;
    assert_extension_then_failure()?;
    super::fold::assert_domain_error_keeps_latest()?;
    assert_policy_observations()?;
    assert_unknown_current()?;
    assert_operation_profile()?;
    Ok(())
}

fn assert_create_transition_cancel() -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let commands = [
        create(b"i", b"q")?,
        transition(b"i", b"q", b"z")?,
        cancel(b"i", 0)?,
    ];
    let plan = accepted(decide(
        &[],
        &[
            (&partition, &commands[0], 0),
            (&partition, &commands[1], 0),
            (&partition, &commands[2], 0),
        ],
        None,
        None,
        AdmissionLimits::default(),
    )?)?;
    assert_eq!(plan.results().len(), 3);
    for index in 0..3 {
        assert_eq!(
            success(plan.results(), index)?.plan().outcome(),
            Outcome::Changed
        );
    }
    assert_eq!(plan.history_plans().count(), 3);
    assert_eq!(plan.final_indices().collect::<Vec<_>>(), [2]);
    Ok(())
}

fn assert_complete_duplicate() -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = claimed(b"i")?;
    let commands = [complete(&record)?, complete(&record)?];
    let plan = accepted(decide(
        &[entry(&partition, &record, 0)],
        &[(&partition, &commands[0], 0), (&partition, &commands[1], 0)],
        None,
        None,
        AdmissionLimits::default(),
    )?)?;
    assert_eq!(plan.results().len(), 2);
    assert_eq!(
        success(plan.results(), 0)?.plan().outcome(),
        Outcome::Changed
    );
    let noop = success(plan.results(), 1)?.plan();
    assert_eq!(noop.outcome(), Outcome::LegacyNoop);
    assert!(noop.write().is_none() && noop.history().is_none());
    assert_eq!(plan.final_indices().collect::<Vec<_>>(), [0]);
    assert_eq!(plan.history_plans().count(), 1);
    assert_eq!(
        write(success(plan.results(), 0)?)?.record.state(),
        b"completed"
    );
    Ok(())
}

fn assert_extension_then_failure() -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = claimed(b"i")?;
    let commands = [
        command(
            b"i",
            Operation::ExtendLease(ExtendLease {
                guard: guard(&record)?,
                lease_ms: 500,
            }),
        )?,
        command(
            b"i",
            Operation::Fail(Terminal {
                guard: guard(&record)?,
                payload: None,
                value: Some(vec![0, 255]),
            }),
        )?,
    ];
    let plan = accepted(decide(
        &[entry(&partition, &record, 0)],
        &[(&partition, &commands[0], 0), (&partition, &commands[1], 0)],
        None,
        None,
        AdmissionLimits::default(),
    )?)?;
    assert_eq!(
        write(success(plan.results(), 0)?)?.record.lease_deadline(),
        TimestampMillis::new(2_500)
    );
    let last = write(success(plan.results(), 1)?)?;
    assert_eq!(last.expected_version, Some(3));
    assert_eq!(last.record.version(), 4);
    assert_eq!(last.record.state(), b"failed");
    assert_eq!(last.record.error(), Some([0, 255].as_slice()));
    assert_eq!(
        plan.history_plans()
            .map(|p| p.plan().history().map(|h| h.kind))
            .collect::<Vec<_>>(),
        [Some(EventKind::LeaseExtended), Some(EventKind::Failed)]
    );
    assert_eq!(
        plan.originals()
            .next()
            .flatten()
            .ok_or("original absent")?
            .record
            .version(),
        2
    );
    Ok(())
}

fn policy_retry(record: &WorkflowRecord) -> TestResult<CommittedCommand> {
    command(
        record.id().as_bytes(),
        Operation::RetryWithPolicy(PolicyRetry {
            guard: guard(record)?,
            run_at: None,
            policy_override: RetryOverride::default(),
            jitter: JitterSample::new(0),
            payload: None,
            error: None,
        }),
    )
}

fn assert_policy_observations() -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = claimed(b"i")?;
    let command = policy_retry(&record)?;
    let installed = InstalledRetryPolicy::new(
        b"r".to_vec(),
        RetryOverride {
            max_retries: Some(0),
            exhausted_to: Some(b"failed".to_vec()),
            ..RetryOverride::default()
        },
        BTreeMap::new(),
    )?;
    for (policy, state) in [
        (Some(&installed), b"failed".as_slice()),
        (None, b"queued".as_slice()),
    ] {
        let plan = accepted(decide(
            &[entry(&partition, &record, 0)],
            &[(&partition, &command, 0)],
            None,
            policy,
            AdmissionLimits::default(),
        )?)?;
        let changed = write(success(plan.results(), 0)?)?;
        assert_eq!(changed.record.state(), state);
        assert_eq!(changed.record.attempts(), 1);
        let event = success(plan.results(), 0)?
            .plan()
            .history()
            .ok_or("history absent")?;
        assert!(event.retry.is_some());
    }
    with_view(
        &[entry(&partition, &record, 0)],
        &[(&partition, &command, 0)],
        None,
        None,
        |view, items| {
            let retries = BTreeMap::new();
            assert_eq!(
                decide_touched_independent_indexed_batch(
                    TouchedIndexedGroupView {
                        retry_policies: &retries,
                        ..view
                    },
                    items,
                    entry_order(7, 5),
                    &[],
                    AdmissionLimits::default()
                ),
                Err(ApplyError::InvalidAuthority)
            );
            let policies = BTreeMap::new();
            assert_eq!(
                decide_touched_independent_indexed_batch(
                    TouchedIndexedGroupView {
                        policies: &policies,
                        ..view
                    },
                    items,
                    entry_order(7, 5),
                    &[],
                    AdmissionLimits::default()
                ),
                Err(ApplyError::InvalidAuthority)
            );
            Ok(())
        },
    )?;
    Ok(())
}

fn assert_operation_profile() -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = initial(b"i", b"queued")?;
    for reclaim in [false, true] {
        let claim = Claim {
            expected_state: b"queued".to_vec(),
            worker: b"w".to_vec(),
            token: LeaseToken::new(b"k".to_vec())?,
            lease_ms: 30_000,
        };
        let command = command(
            b"i",
            if reclaim {
                Operation::Reclaim(claim)
            } else {
                Operation::Claim(claim)
            },
        )?;
        assert_eq!(
            decide(
                &[entry(&partition, &record, 0)],
                &[(&partition, &command, 0)],
                None,
                None,
                AdmissionLimits::default()
            )?,
            IndependentIndexedDecision::Rejected(rejected(DomainError::UnsupportedOperation))
        );
    }
    Ok(())
}

fn assert_unknown_current() -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = claimed(b"i")?;
    let command = policy_retry(&record)?;
    with_view(
        &[entry(&partition, &record, 0)],
        &[(&partition, &command, 0)],
        None,
        None,
        |view, items| {
            let current = BTreeMap::new();
            assert_eq!(
                decide_touched_independent_indexed_batch(
                    TouchedIndexedGroupView {
                        current: &current,
                        ..view
                    },
                    items,
                    entry_order(7, 5),
                    &[],
                    AdmissionLimits::default()
                ),
                Err(ApplyError::InvalidAuthority)
            );
            Ok(())
        },
    )
}

fn owned_chain(cap: usize, payload: bool) -> TestResult<Vec<Result<ScopedPlan, IndexRejection>>> {
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = claimed(b"i")?;
    let commands = [
        retry(&record, payload.then(|| vec![7; 256]))?,
        cancel(b"i", 1)?,
    ];
    let plan = accepted(decide(
        &[entry(&partition, &record, 0)],
        &[(&partition, &commands[0], 0), (&partition, &commands[1], 0)],
        None,
        None,
        AdmissionLimits::new(256, cap, 2)?,
    )?)?;
    // Consuming transfer removes all original descriptors before donors drop.
    Ok(plan.into_results())
}

#[test]
fn independent_indexed_limits_lifetimes_and_publication_slots() -> TestResult {
    // Source grammar only: V32/544 with fixed5376; four-step V24/fixed10752.
    for (cap, payload) in [(5_504, false), (7_552, true)] {
        let results = owned_chain(cap, payload)?;
        assert_eq!(results.len(), 2);
        for index in [0, 1] {
            let record = &write(success(&results, index)?)?.record;
            assert_eq!(record.payload(), payload.then_some([7; 256].as_slice()));
            assert_eq!(record.id().as_bytes(), b"i");
            assert_eq!(record.workflow_type(), b"r");
        }
    }
    assert_late_whole_refusals()?;
    assert_query_lifetime_and_slots()?;
    assert_empty_and_legacy_limits()?;
    assert_ordinal_overflow()?;
    Ok(())
}

fn assert_late_whole_refusals() -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = claimed(b"i")?;
    for (cap, payload) in [(5_503, false), (7_551, true)] {
        let commands = [
            retry(&record, payload.then(|| vec![7; 256]))?,
            cancel(b"i", 1)?,
        ];
        assert_eq!(
            decide(
                &[entry(&partition, &record, 0)],
                &[(&partition, &commands[0], 0), (&partition, &commands[1], 0)],
                None,
                None,
                AdmissionLimits::new(256, cap, 2)?
            )?,
            IndependentIndexedDecision::Rejected(IndexRejection::Batch(BatchRejection::Admission(
                AdmissionError::DecisionBytes
            )))
        );
    }
    let policy = policy(&[(b"q", StateMode::Fifo)])?;
    let commands = [
        create(b"i", b"q")?,
        create(b"i", b"q")?,
        transition(b"i", b"q", b"z")?,
        cancel(b"i", 0)?,
    ];
    assert_eq!(
        decide(
            &[],
            &[
                (&partition, &commands[0], 1),
                (&partition, &commands[1], 0),
                (&partition, &commands[2], 0),
                (&partition, &commands[3], 0)
            ],
            Some(policy.policy()),
            None,
            AdmissionLimits::new(256, 10_847, 4)?
        )?,
        IndependentIndexedDecision::Rejected(IndexRejection::Batch(BatchRejection::Admission(
            AdmissionError::DecisionBytes
        )))
    );
    Ok(())
}

fn assert_query_lifetime_and_slots() -> TestResult {
    let p = PartitionId::new(b"p".to_vec())?;
    let q = PartitionId::new(b"q".to_vec())?;
    let plan = {
        let commands = [create(b"i", b"q")?, create(b"i", b"q")?, cancel(b"i", 0)?];
        accepted(decide(
            &[],
            &[
                (&q, &commands[0], 0),
                (&p, &commands[1], 0),
                (&p, &commands[2], 0),
            ],
            None,
            None,
            AdmissionLimits::default(),
        )?)?
    };
    // Commands and all query/fact maps have dropped; input order differs from keys.
    assert_eq!(plan.final_indices().collect::<Vec<_>>(), [2, 0]);
    assert_eq!(plan.originals().collect::<Vec<_>>(), [None, None]);
    assert_eq!(
        plan.metadata()
            .map(|m| m.state_enter_seq)
            .collect::<Vec<_>>(),
        [(7_u128 << 32) + 6, (7_u128 << 32) + 5]
    );
    let deltas = plan.deltas().collect::<Result<Vec<_>, _>>()?;
    assert_eq!(deltas.len(), 2);
    assert!(deltas[0].before.is_none() && deltas[0].after.is_none());
    assert!(
        matches!(deltas[1].after, Some(IndexRows::Parallel { due }) if due.partition_key == b"q" && due.id == b"i")
    );
    assert_eq!(write(success(plan.results(), 0)?)?.record.state(), b"q");
    assert_eq!(
        write(success(plan.results(), 2)?)?.record.state(),
        b"cancelled"
    );
    assert_eq!(plan.history_plans().count(), 3);
    Ok(())
}

fn assert_empty_and_legacy_limits() -> TestResult {
    let empty = accepted(decide(&[], &[], None, None, AdmissionLimits::default())?)?;
    assert_eq!(
        empty.results(),
        [] as [Result<ScopedPlan, IndexRejection>; 0]
    );
    assert_eq!(empty.consumed_ordinals(), 0);
    assert_eq!(empty.next_ordinal(), Some(5));
    assert_eq!(empty.final_indices().len(), 0);
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = claimed(b"i")?;
    let commands = [complete(&record)?, complete(&record)?];
    with_view(
        &[entry(&partition, &record, 0)],
        &[(&partition, &commands[0], 0), (&partition, &commands[1], 0)],
        None,
        None,
        |view, items| {
            let rows = BTreeMap::from([(
                ScopedRecordId {
                    partition_key: b"p",
                    id: b"i",
                },
                &record,
            )]);
            let batch = GroupView {
                group: view.group,
                placements: view.placements,
                records: &rows,
            };
            let items: Vec<_> = items.iter().map(|i| i.item).collect();
            assert!(matches!(
                decide_independent_terminal_batch(
                    batch,
                    &items,
                    &[],
                    AdmissionLimits::new(256, 2_472, 2)?
                )?,
                IndependentTransitionDecision::Accepted(_)
            ));
            assert_eq!(
                decide_independent_transition_batch(
                    batch,
                    &items,
                    &[],
                    AdmissionLimits::default()
                )?,
                IndependentTransitionDecision::Rejected(BatchRejection::Domain(
                    DomainError::UnsupportedOperation
                ))
            );
            Ok(())
        },
    )?;
    Ok(())
}

fn assert_ordinal_overflow() -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let commands = [create(b"i", b"q")?, transition(b"i", b"q", b"z")?];
    with_view(
        &[],
        &[(&partition, &commands[0], 0), (&partition, &commands[1], 0)],
        None,
        None,
        |view, items| {
            assert_eq!(
                decide_touched_independent_indexed_batch(
                    view,
                    items,
                    entry_order(7, u32::MAX),
                    &[],
                    AdmissionLimits::default()
                ),
                Err(ApplyError::InvalidAuthority)
            );
            Ok(())
        },
    )
}
