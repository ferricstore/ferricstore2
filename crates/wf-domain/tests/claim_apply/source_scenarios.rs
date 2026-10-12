//! Source-observation scenarios plus supplemental scoped-identity composition.

use super::*;

#[test]
fn priority_phase_writes_in_due_order() -> TestResult {
    // Priority source scenarios 9bdbecf61fe86d5f/7d7eaf003a23555b, represented fields.
    let scope = test_scope()?;
    let request = RequestId::new(b"priority".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let rows = [
        test_record(b"low", b"job", b"queued", None, 0, 1)?,
        test_record(b"high-b", b"job", b"queued", None, 0, 2)?,
        test_record(b"high-a", b"job", b"queued", None, 0, 2)?,
    ];
    let entries: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(i, record)| ClaimRecord {
            record,
            partition_key: b"p",
            priority: if i == 0 { 0 } else { 2 },
            state_enter_seq: i as u128,
        })
        .collect();
    let outcomes = BTreeMap::new();
    with_claim_view(
        &entries,
        &[partition],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::new(),
        1,
        |view| {
            let plan = decide_claim_request(
                view,
                &test_envelope(test_header(&scope, &request, 1, 25)?, test_query(b"job", 3)),
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision() else {
                return Err("expected accepted priority phase".into());
            };
            let batch = claim.batch().ok_or("missing priority writes")?;
            assert_eq!(
                batch
                    .batch()
                    .items()
                    .iter()
                    .map(|item| item
                        .plan()
                        .write()
                        .map(|write| write.record.id().as_bytes()))
                    .collect::<Vec<_>>(),
                [
                    Some(b"high-a".as_slice()),
                    Some(b"high-b".as_slice()),
                    Some(b"low".as_slice())
                ]
            );
            assert_eq!(batch.consumed_ordinals(), 0);
            assert_eq!(batch.next_ordinal(), Some(u32::MAX));
            assert_eq!(
                batch
                    .metadata()
                    .map(|meta| meta.priority)
                    .collect::<Vec<_>>(),
                [2, 2, 0]
            );
            assert_eq!(claim.stop(), SelectionStop::Complete);
            assert!(plan.requires_outcome_write());
            Ok(())
        },
    )
}

#[test]
fn physical_running_phase_reclaims_only_expired_records() -> TestResult {
    let scope = test_scope()?;
    let request = RequestId::new(b"reclaim".to_vec())?;
    let queued = test_record(b"expired", b"job", b"queued", None, 0, 0)?;
    let expired = test_claimed(&queued, b"old", 0, 10)?;
    let queued_live = test_record(b"live", b"job", b"queued", None, 0, 0)?;
    let live = test_claimed(&queued_live, b"old", 0, 100)?;
    let scheduled = test_record(b"plain", b"job", b"queued", None, 0, 0)?;
    let entries = [&expired, &live, &scheduled].map(|record| ClaimRecord {
        record,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: 11,
    });
    let outcomes = BTreeMap::new();
    let physical = [b"running".as_slice()];
    with_claim_view(
        &entries,
        &[PartitionId::new(b"p".to_vec())?],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::new(),
        1,
        |view| {
            let query = ClaimQuery {
                states: ClaimStates::Only(&physical),
                ..test_query(b"job", 3)
            };
            let plan = decide_claim_request(
                view,
                &test_envelope(test_header(&scope, &request, 1, 25)?, query),
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision() else {
                return Err("expected physical reclaim".into());
            };
            let batch = claim.batch().ok_or("missing reclaim")?;
            assert_eq!(batch.batch().items().len(), 1);
            let item = &batch.batch().items()[0];
            let record = &item
                .plan()
                .write()
                .ok_or("missing reclaimed record")?
                .record;
            assert_eq!(record.id().as_bytes(), b"expired");
            assert_eq!(
                (record.version(), record.fence().get(), record.attempts()),
                (3, 2, 0)
            );
            assert_eq!(record.run_state(), Some(b"queued".as_slice()));
            assert_eq!(
                record.lease().ok_or("no lease")?.token.as_bytes(),
                b"worker-a:25:2"
            );
            assert_eq!(record.lease_deadline(), TimestampMillis::new(75));
            assert_eq!(
                item.plan()
                    .history()
                    .ok_or("no reclaim history")?
                    .worker
                    .as_deref(),
                Some(b"worker-a".as_slice())
            );
            assert_eq!(live.version(), 2);
            assert_eq!(scheduled.version(), 1);
            Ok(())
        },
    )
}

#[test]
fn deep_fifo_heads_produce_atomic_lease_and_index_plans() -> TestResult {
    // Source80-follower scenarios74f5fcdcf4328003/9f5d395534240b02, no follower skip.
    let scope = test_scope()?;
    let request = RequestId::new(b"fifo".to_vec())?;
    let rows: Vec<_> = (0..81)
        .map(|i| {
            test_record(
                format!("row-{i:03}").as_bytes(),
                b"job",
                b"queued",
                None,
                10,
                10,
            )
        })
        .collect::<Result<_, _>>()?;
    let entries: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(i, record)| ClaimRecord {
            record,
            partition_key: b"p",
            priority: 0,
            state_enter_seq: 11 + i as u128,
        })
        .collect();
    let policy = test_fifo_policy(&[b"queued"])?;
    let policies = BTreeMap::from([(b"job".as_slice(), policy.policy())]);
    let outcomes = BTreeMap::new();
    let (head, control) = with_claim_view(
        &entries,
        &[PartitionId::new(b"p".to_vec())?],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &policies,
        1,
        |view| {
            let plan = decide_claim_request(
                view,
                &test_envelope(
                    test_header(&scope, &request, 1, 25)?,
                    test_query(b"job", 81),
                ),
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision() else {
                return Err("expected FIFO claim".into());
            };
            let batch = claim.batch().ok_or("no FIFO batch")?;
            assert_eq!(
                (
                    claim.head_reads(),
                    claim.scanned(),
                    batch.batch().items().len()
                ),
                (1, 1, 1)
            );
            assert_eq!(
                batch.metadata().next().ok_or("no order")?.state_enter_seq,
                11
            );
            let write = batch.batch().items()[0]
                .plan()
                .write()
                .ok_or("no head write")?;
            assert_eq!(write.record.id().as_bytes(), b"row-000");
            assert_eq!((write.record.version(), write.record.fence().get()), (2, 1));
            assert_eq!(
                write.record.lease().ok_or("no lease")?.token.as_bytes(),
                b"worker-a:25:1"
            );
            assert_eq!(write.record.created_at(), TimestampMillis::new(10));
            assert_eq!(write.record.next_run_at(), Some(TimestampMillis::new(75)));
            assert!(
                batch
                    .deltas()
                    .all(|delta| delta.before.is_some() && delta.after.is_some())
            );
            Ok((write.record.clone(), plan.control().next()))
        },
    )?;
    check_fifo_live_and_reclaim(
        &head,
        &rows[1..],
        GateView {
            scope: &scope,
            control,
            outcomes: &outcomes,
        },
        &policies,
    )
}

#[test]
fn source_token_preserves_binary_worker_and_exact_decimal_identity() -> TestResult {
    // ordered_index45306a7c90f87fd6 exactworker-a:25:1 is also asserted above.
    let scope = test_scope()?;
    let request = RequestId::new(b"binary".to_vec())?;
    let row = test_record(b"id", b"job", b"queued", None, 10, 10)?;
    let entries = [ClaimRecord {
        record: &row,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: 11,
    }];
    let outcomes = BTreeMap::new();
    with_claim_view(
        &entries,
        &[PartitionId::new(b"p".to_vec())?],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::new(),
        1,
        |view| {
            let query = ClaimQuery {
                worker: &[0, 255, b':'],
                ..test_query(b"job", 1)
            };
            let mut header = test_header(&scope, &request, 1, 25)?;
            header.stamp = test_stamp(25, 20)?;
            let plan = decide_claim_request(
                view,
                &test_envelope(header, query),
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision() else {
                return Err("expected binary worker".into());
            };
            let item = &claim.batch().ok_or("no batch")?.batch().items()[0];
            let record = &item.plan().write().ok_or("no record")?.record;
            assert_eq!(
                record.lease().ok_or("no lease")?.token.as_bytes(),
                &[0, 255, b':', b':', b'2', b'5', b':', b'1']
            );
            assert_eq!(item.plan().history().ok_or("no history")?.event_id, "25-2");
            assert_eq!(record.updated_at(), TimestampMillis::new(25));
            Ok(())
        },
    )
}

#[test]
fn same_record_id_in_distinct_partitions_is_claimed_as_two_scoped_records() -> TestResult {
    // Supplemental keys.ex20–28/per-candidate source composition, no frozen case credit.
    let scope = test_scope()?;
    let request = RequestId::new(b"scoped".to_vec())?;
    let a = test_record(b"same", b"job", b"queued", None, 0, 0)?;
    let b = test_record(b"same", b"job", b"queued", None, 0, 0)?;
    let entries = [
        ClaimRecord {
            record: &a,
            partition_key: b"p",
            priority: 0,
            state_enter_seq: 1,
        },
        ClaimRecord {
            record: &b,
            partition_key: b"q",
            priority: 0,
            state_enter_seq: 2,
        },
    ];
    let partitions = [
        PartitionId::new(b"p".to_vec())?,
        PartitionId::new(b"q".to_vec())?,
    ];
    let outcomes = BTreeMap::new();
    with_claim_view(
        &entries,
        &partitions,
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::new(),
        1,
        |view| {
            let plan = decide_claim_request(
                view,
                &test_envelope(test_header(&scope, &request, 1, 25)?, test_query(b"job", 2)),
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision() else {
                return Err("scoped IDs were conflated".into());
            };
            let batch = claim.batch().ok_or("no scoped writes")?;
            assert_eq!(
                batch
                    .batch()
                    .items()
                    .iter()
                    .map(|item| item.partition().as_bytes())
                    .collect::<Vec<_>>(),
                [b"p".as_slice(), b"q".as_slice()]
            );
            assert!(batch.batch().items().iter().all(|item| {
                item.plan()
                    .write()
                    .is_some_and(|write| write.record.fence().get() == 1)
            }));
            assert_eq!(batch.consumed_ordinals(), 0);
            Ok(())
        },
    )
}

#[test]
fn empty_phase_finalizes_request_without_record_writes() -> TestResult {
    let scope = test_scope()?;
    let request = RequestId::new(b"empty".to_vec())?;
    let outcomes = BTreeMap::new();
    with_claim_view(
        &[],
        &[],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::new(),
        1,
        |view| {
            let plan = decide_claim_request(
                view,
                &test_envelope(test_header(&scope, &request, 1, 25)?, test_query(b"job", 1)),
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision() else {
                return Err("empty was not accepted".into());
            };
            assert!(claim.batch().is_none());
            assert_eq!(
                (claim.scanned(), claim.head_reads(), claim.stop()),
                (0, 0, SelectionStop::Complete)
            );
            assert!(plan.requires_outcome_write());
            assert_eq!(plan.control().next().position(), AppliedPosition::new(1));
            Ok(())
        },
    )
}

#[test]
fn consumed_authority_failure_discards_every_provisional_write() -> TestResult {
    let scope = test_scope()?;
    let request = RequestId::new(b"bad-late".to_vec())?;
    let good = test_record(b"a", b"job", b"queued", None, 0, 0)?;
    let bad = test_record(b"z", b"job", b"queued", None, 0, 0)?;
    let entries = [&good, &bad].map(|record| ClaimRecord {
        record,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: 1,
    });
    let before = [&good, &bad].map(Clone::clone);
    let outcomes = BTreeMap::new();
    with_claim_view(
        &entries,
        &[PartitionId::new(b"p".to_vec())?],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::new(),
        1,
        |view| {
            let mut canonical = view.group.batch.records.clone();
            canonical.remove(&ScopedRecordId {
                partition_key: b"p",
                id: b"z",
            });
            let corrupt = ClaimApplyView {
                group: IndexedGroupView {
                    batch: GroupView {
                        records: &canonical,
                        ..view.group.batch
                    },
                    ..view.group
                },
                ..view
            };
            assert!(matches!(
                decide_claim_request(
                    corrupt,
                    &test_envelope(test_header(&scope, &request, 1, 25)?, test_query(b"job", 2)),
                    AdmissionLimits::default(),
                    ClaimWorkLimits::default()
                ),
                Err(ApplyError::InvalidAuthority)
            ));
            assert_eq!((&good, &bad), (&before[0], &before[1]));
            Ok(())
        },
    )
}

fn check_fifo_live_and_reclaim(
    record: &WorkflowRecord,
    followers: &[WorkflowRecord],
    gate: GateView<'_>,
    policies: &BTreeMap<&[u8], &PolicySnapshot>,
) -> TestResult {
    let entries = std::iter::once(record)
        .chain(followers)
        .enumerate()
        .map(|(index, record)| ClaimRecord {
            record,
            partition_key: b"p",
            priority: 0,
            state_enter_seq: 11 + index as u128,
        })
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 81);
    let live_id = RequestId::new(b"fifo-live".to_vec())?;
    with_claim_view(
        &entries,
        &[PartitionId::new(b"p".to_vec())?],
        gate,
        policies,
        2,
        |view| {
            let plan = decide_claim_request(
                view,
                &test_envelope(
                    test_header(gate.scope, &live_id, 2, 26)?,
                    test_query(b"job", 81),
                ),
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision() else {
                return Err("live FIFO head was not empty acceptance".into());
            };
            assert!(claim.batch().is_none());
            assert_eq!(
                (claim.head_reads(), claim.scanned(), claim.stop()),
                (1, 1, SelectionStop::Complete)
            );
            assert!(plan.requires_outcome_write());
            let next_view = ClaimApplyView {
                gate: GateView {
                    control: plan.control().next(),
                    ..view.gate
                },
                ..view
            };
            let reclaim_id = RequestId::new(b"fifo-reclaim".to_vec())?;
            let physical = [b"running".as_slice()];
            let query = ClaimQuery {
                states: ClaimStates::Only(&physical),
                ..test_query(b"job", 81)
            };
            let plan = decide_claim_request(
                next_view,
                &test_envelope(test_header(gate.scope, &reclaim_id, 3, 75)?, query),
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            check_fifo_reclaim_plan(&plan, record, followers)
        },
    )
}

fn check_fifo_reclaim_plan(
    plan: &DispatchPlan<'_, ClaimDecision<'_>>,
    record: &WorkflowRecord,
    followers: &[WorkflowRecord],
) -> TestResult {
    let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision() else {
        return Err("FIFO head reclaim rejected".into());
    };
    let batch = claim.batch().ok_or("no reclaimed FIFO head")?;
    assert_eq!(
        (
            claim.head_reads(),
            claim.scanned(),
            batch.batch().items().len()
        ),
        (1, 1, 1)
    );
    let item = &batch.batch().items()[0];
    let next = &item.plan().write().ok_or("no FIFO reclaim write")?.record;
    assert_eq!(next.id().as_bytes(), b"row-000");
    assert_eq!(
        (next.version(), next.fence().get(), next.attempts()),
        (3, 2, 0)
    );
    assert_eq!(
        next.lease()
            .ok_or("no FIFO reclaim lease")?
            .token
            .as_bytes(),
        b"worker-a:75:2"
    );
    assert_eq!(next.lease_deadline(), TimestampMillis::new(125));
    assert_eq!(next.run_state(), Some(b"queued".as_slice()));
    assert_eq!(
        batch
            .metadata()
            .next()
            .ok_or("no FIFO reclaim order")?
            .state_enter_seq,
        11
    );
    assert_eq!(
        (batch.consumed_ordinals(), batch.next_ordinal()),
        (0, Some(u32::MAX))
    );
    let history = item.plan().history().ok_or("no FIFO reclaim history")?;
    assert_eq!(
        (history.version, history.fence.get(), history.attempts),
        (3, 2, 0)
    );
    assert_eq!(history.worker.as_deref(), Some(b"worker-a".as_slice()));
    assert_eq!(record.version(), 2);
    assert!(followers.iter().all(|record| record.version() == 1));
    Ok(())
}
