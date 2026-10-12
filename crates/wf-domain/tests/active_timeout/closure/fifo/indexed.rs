//! Generic core-only indexed updates of a real unscheduled FIFO owner.
use super::*;
use wf_domain::claim::Claim;
use wf_domain::workflow::admission::AdmissionError;
use wf_domain::workflow::batch::{BatchItem, BatchRejection};
use wf_domain::workflow::indexed::{
    IndexRejection, IndexRows, IndexedBatchItem, IndexedBatchPlan, IndexedDecision,
    decide_indexed_batch,
};
use wf_domain::workflow::{
    Cancel, CommittedCommand, Create, DomainError, Operation, Outcome, Transition, WorkflowRecord,
    reduce,
};

pub(super) fn assert_generic_unscheduled_fifo_deltas(
    policies: &BTreeMap<Vec<u8>, PolicySnapshot>,
) -> TestResult {
    let captured = fixtures::lifecycle(MaxActiveOverride::Infinite, 100, 0, 10)?;
    let zero = TimestampMillis::new(0);
    let records = [restore_active(
        fixture(b"r", b"p", b"waiting", zero, None, captured)?,
        AdmissionLimits::default(),
    )?];
    let originals = records.clone();
    let order = EntryOrder {
        identity: LogIdentity::new(1, 7, AppliedPosition::new(2)),
        first_ordinal: 4,
    };
    with_view(
        &records,
        policies,
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            for (to, kind, ordinals) in [
                (b"cancelled".as_slice(), EventKind::Cancelled, 0),
                (b"queued", EventKind::Transitioned, 1),
            ] {
                let operation = if ordinals == 0 {
                    Operation::Cancel(Cancel {
                        fence: Fence::new(1),
                        lease_token: None,
                        expected_state: Some(b"waiting".to_vec()),
                        expected_version: Some(1),
                        error: None,
                    })
                } else {
                    Operation::Transition(Transition {
                        from: b"waiting".to_vec(),
                        to: to.to_vec(),
                        fence: Fence::new(1),
                        lease_token: None,
                        run_at: None,
                        payload: None,
                    })
                };
                let command = CommittedCommand {
                    id: records[0].core().id().clone(),
                    now: TimestampMillis::new(1),
                    operation,
                };
                let item =
                    IndexedBatchItem::new(BatchItem::new(records[0].partition(), &command, b"key"));
                let IndexedDecision::Accepted(plan) = decide_indexed_batch(
                    view.indexed,
                    &[item],
                    order,
                    b"entry",
                    AdmissionLimits::default(),
                )?
                else {
                    return Err("generic unscheduled update rejected".into());
                };
                assert_eq!(plan.batch().items().len(), 1);
                let deltas: Vec<_> = plan.deltas().collect();
                assert_eq!(deltas.len(), 1);
                assert!(
                    deltas[0].before.is_some(),
                    "missing FIFO before: {:?}",
                    deltas[0]
                );
                let Some(IndexRows::Fifo {
                    lane,
                    member,
                    head_candidate,
                }) = deltas[0].before
                else {
                    return Err("unscheduled before is not FIFO".into());
                };
                let entry = view.indexed.records[&ScopedRecordId {
                    partition_key: b"p",
                    id: b"r",
                }];
                assert_eq!(Some(lane), LaneKey::for_record(&entry));
                assert_eq!(member, LaneOrder::for_record(&entry));
                assert!(head_candidate.is_none());
                if ordinals == 0 {
                    assert!(deltas[0].after.is_none());
                } else {
                    assert!(
                        matches!(deltas[0].after, Some(IndexRows::Parallel { due }) if due.due == TimestampMillis::new(1) && due.state == b"queued")
                    );
                }
                assert_generic_mutation(&plan, to, kind, ordinals)?;
            }
            Ok(())
        },
    )?;
    assert!(records == originals);
    assert_eq!(records[0].core().version(), 1);
    assert_eq!(records[0].core().fence(), Fence::new(1));
    assert_eq!(records[0].core().next_run_at(), None);
    Ok(())
}

pub(super) fn assert_generic_scheduled_fifo_entry(
    policies: &BTreeMap<Vec<u8>, PolicySnapshot>,
) -> TestResult {
    let captured = fixtures::lifecycle(MaxActiveOverride::Infinite, 100, 0, 10)?;
    let zero = TimestampMillis::new(0);
    let records = [restore_active(
        fixture(b"r", b"p", b"queued", zero, Some(zero), captured)?,
        AdmissionLimits::default(),
    )?];
    let originals = records.clone();
    let command = CommittedCommand {
        id: records[0].core().id().clone(),
        now: TimestampMillis::new(1),
        operation: Operation::Transition(Transition {
            from: b"queued".to_vec(),
            to: b"waiting".to_vec(),
            fence: Fence::new(1),
            lease_token: None,
            run_at: None,
            payload: None,
        }),
    };
    let order = EntryOrder {
        identity: LogIdentity::new(1, 7, AppliedPosition::new(2)),
        first_ordinal: 4,
    };
    with_view(
        &records,
        policies,
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let item =
                IndexedBatchItem::new(BatchItem::new(records[0].partition(), &command, b"key"));
            let IndexedDecision::Accepted(plan) = decide_indexed_batch(
                view.indexed,
                &[item],
                order,
                b"entry",
                AdmissionLimits::default(),
            )?
            else {
                return Err("scheduled FIFO entry rejected".into());
            };
            assert_eq!(plan.batch().items().len(), 1);
            let deltas: Vec<_> = plan.deltas().collect();
            assert_eq!(deltas.len(), 1);
            let Some(IndexRows::Parallel { due: before }) = deltas[0].before else {
                return Err("parallel before absent".into());
            };
            let Some(IndexRows::Fifo {
                lane,
                member,
                head_candidate,
            }) = deltas[0].after
            else {
                return Err("FIFO after absent".into());
            };
            let due = head_candidate.ok_or("scheduled FIFO due absent")?;
            assert_eq!(
                (before.due, due.due, member.score()),
                (zero, TimestampMillis::new(1), 0)
            );
            assert_generic_mutation(&plan, b"waiting", EventKind::Transitioned, 1)?;
            let metadata = plan.metadata().next().ok_or("metadata")?;
            assert_eq!(
                (metadata.priority, metadata.state_enter_seq),
                (0, (2_u128 << 32) + 4)
            );
            let scoped = &plan.batch().items()[0];
            let write = scoped.plan().write().ok_or("write")?;
            assert!(std::ptr::eq(
                before.workflow_type,
                records[0].core().workflow_type()
            ));
            assert!(std::ptr::eq(
                lane.partition_key,
                scoped.partition().as_bytes()
            ));
            assert!(std::ptr::eq(due.state, write.record.state()));
            assert!(std::ptr::eq(
                due.workflow_type,
                write.record.workflow_type()
            ));
            Ok(())
        },
    )?;
    assert!(records == originals);
    Ok(())
}

pub(super) fn assert_generic_unscheduled_fifo_authority(
    policies: &BTreeMap<Vec<u8>, PolicySnapshot>,
) -> TestResult {
    let captured = fixtures::lifecycle(MaxActiveOverride::Infinite, 100, 0, 10)?;
    let zero = TimestampMillis::new(0);
    let records = [b"r".as_slice(), b"s"]
        .into_iter()
        .map(|id| {
            Ok(restore_active(
                fixture(id, b"p", b"waiting", zero, None, captured)?,
                AdmissionLimits::default(),
            )?)
        })
        .collect::<TestResult<Vec<_>>>()?;
    let originals = records.clone();
    let command = CommittedCommand {
        id: records[0].core().id().clone(),
        now: TimestampMillis::new(1),
        operation: Operation::Cancel(Cancel {
            fence: Fence::new(1),
            lease_token: None,
            expected_state: Some(b"waiting".to_vec()),
            expected_version: Some(1),
            error: None,
        }),
    };
    let mirror = reduce(
        None,
        &CommittedCommand {
            id: command.id.clone(),
            now: zero,
            operation: Operation::Create(Create {
                workflow_type: b"job".to_vec(),
                state: b"waiting".to_vec(),
                run_at: Some(zero),
                payload: Some(vec![7; 4096]),
            }),
        },
    )?
    .into_parts()
    .1
    .ok_or("mirror write")?
    .record;
    let order = EntryOrder {
        identity: LogIdentity::new(1, 7, AppliedPosition::new(2)),
        first_ordinal: 4,
    };
    with_view(
        &records,
        policies,
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let item =
                IndexedBatchItem::new(BatchItem::new(records[0].partition(), &command, b"key"));
            assert_generic_bad_mirrors(view.indexed, item, &mirror, order)?;
            let mut second = command.clone();
            second.id = records[1].core().id().clone();
            let Operation::Cancel(cancel) = &mut second.operation else {
                return Err("cancel fixture".into());
            };
            cancel.expected_state = Some(b"queued".to_vec());
            let items = [
                item,
                IndexedBatchItem::new(BatchItem::new(records[1].partition(), &second, b"key-s")),
            ];
            assert_eq!(
                decide_indexed_batch(
                    view.indexed,
                    &items,
                    order,
                    b"entry",
                    AdmissionLimits::default()
                )?,
                IndexedDecision::Rejected(IndexRejection::Batch(BatchRejection::Domain(
                    DomainError::WrongState
                )))
            );
            assert!(std::ptr::eq(
                view.indexed.records[&ScopedRecordId {
                    partition_key: b"p",
                    id: b"r"
                }]
                    .record,
                records[0].core()
            ));
            Ok(())
        },
    )?;
    assert!(records == originals);
    Ok(())
}

pub(super) fn assert_generic_unscheduled_fifo_noops(
    policies: &BTreeMap<Vec<u8>, PolicySnapshot>,
) -> TestResult {
    let captured = fixtures::lifecycle(MaxActiveOverride::Infinite, 100, 0, 10)?;
    let zero = TimestampMillis::new(0);
    let records = [restore_active(
        fixture(b"r", b"p", b"waiting", zero, None, captured)?,
        AdmissionLimits::default(),
    )?];
    let originals = records.clone();
    let order = EntryOrder {
        identity: LogIdentity::new(1, 7, AppliedPosition::new(2)),
        first_ordinal: 4,
    };
    with_view(
        &records,
        policies,
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            for (operation, outcome) in [
                (
                    Operation::Transition(Transition {
                        from: b"waiting".to_vec(),
                        to: b"waiting".to_vec(),
                        fence: Fence::new(1),
                        lease_token: None,
                        run_at: None,
                        payload: None,
                    }),
                    Outcome::LegacyNoop,
                ),
                (
                    Operation::Claim(Claim {
                        expected_state: b"waiting".to_vec(),
                        worker: b"worker".to_vec(),
                        token: LeaseToken::new(b"token".to_vec())?,
                        lease_ms: 10,
                    }),
                    Outcome::NotEligible,
                ),
            ] {
                let command = CommittedCommand {
                    id: records[0].core().id().clone(),
                    now: TimestampMillis::new(1),
                    operation,
                };
                let item =
                    IndexedBatchItem::new(BatchItem::new(records[0].partition(), &command, b"key"));
                let IndexedDecision::Accepted(plan) = decide_indexed_batch(
                    view.indexed,
                    &[item],
                    order,
                    b"entry",
                    AdmissionLimits::default(),
                )?
                else {
                    return Err("generic no-op rejected".into());
                };
                assert_eq!(plan.batch().items().len(), 1);
                let transition = plan.batch().items()[0].plan();
                assert_eq!(transition.outcome(), outcome);
                assert!(transition.write().is_none() && transition.history().is_none());
                let deltas: Vec<_> = plan.deltas().collect();
                assert_eq!(deltas.len(), 1);
                assert!(deltas[0].before.is_none() && deltas[0].after.is_none());
                assert_eq!(
                    (plan.consumed_ordinals(), plan.next_ordinal()),
                    (0, Some(4))
                );
                assert_eq!(plan.metadata().next().ok_or("metadata")?.state_enter_seq, 0);
            }
            Ok(())
        },
    )?;
    assert!(records == originals);
    assert_eq!(records[0].core().next_run_at(), None);
    Ok(())
}

pub(super) fn assert_generic_unscheduled_fifo_budget(
    policies: &BTreeMap<Vec<u8>, PolicySnapshot>,
) -> TestResult {
    let captured = fixtures::lifecycle(MaxActiveOverride::Infinite, 100, 0, 10)?;
    let zero = TimestampMillis::new(0);
    let records = [restore_active(
        fixture(b"r", b"p", b"waiting", zero, None, captured)?,
        AdmissionLimits::default(),
    )?];
    let originals = records.clone();
    let command = CommittedCommand {
        id: records[0].core().id().clone(),
        now: TimestampMillis::new(1),
        operation: Operation::Cancel(Cancel {
            fence: Fence::new(1),
            lease_token: None,
            expected_state: Some(b"waiting".to_vec()),
            expected_version: None,
            error: None,
        }),
    };
    let order = EntryOrder {
        identity: LogIdentity::new(1, 7, AppliedPosition::new(2)),
        first_ordinal: 4,
    };
    with_view(
        &records,
        policies,
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let item =
                IndexedBatchItem::new(BatchItem::new(records[0].partition(), &command, b"key"));
            for maximum in [1204, 1203] {
                let limits = AdmissionLimits::new(1_048_576, maximum, 1)?;
                let decision =
                    decide_indexed_batch(view.indexed, &[item], order, b"entry", limits)?;
                if maximum == 1204 {
                    let IndexedDecision::Accepted(plan) = decision else {
                        return Err("exact generic budget rejected".into());
                    };
                    assert_eq!(plan.batch().items().len(), 1);
                    assert!(plan.batch().items()[0].plan().write().is_some());
                    assert_eq!(plan.consumed_ordinals(), 0);
                } else {
                    assert_eq!(
                        decision,
                        IndexedDecision::Rejected(IndexRejection::Batch(
                            BatchRejection::Admission(AdmissionError::DecisionBytes)
                        ))
                    );
                }
            }
            Ok(())
        },
    )?;
    assert!(records == originals);
    Ok(())
}

fn assert_generic_mutation(
    plan: &IndexedBatchPlan<'_>,
    to: &[u8],
    kind: EventKind,
    ordinals: u32,
) -> TestResult {
    let scoped = &plan.batch().items()[0];
    let write = scoped.plan().write().ok_or("write absent")?;
    let history = scoped.plan().history().ok_or("history absent")?;
    assert_eq!(
        (
            write.expected_version,
            write.record.version(),
            write.record.fence()
        ),
        (Some(1), 2, Fence::new(1))
    );
    assert_eq!(write.record.state(), to);
    assert!(write.record.lease().is_none());
    assert_eq!(
        (history.kind, history.version, history.fence),
        (kind, 2, Fence::new(1))
    );
    assert_eq!(
        (plan.consumed_ordinals(), plan.next_ordinal()),
        (ordinals, Some(4 + ordinals))
    );
    assert_eq!(
        plan.metadata().next().ok_or("metadata")?.state_enter_seq,
        if ordinals == 0 { 0 } else { (2_u128 << 32) + 4 }
    );
    Ok(())
}

fn assert_generic_bad_mirrors(
    view: IndexedGroupView<'_>,
    item: IndexedBatchItem<'_>,
    mirror: &WorkflowRecord,
    order: EntryOrder,
) -> TestResult {
    let entry = view.records[&ScopedRecordId {
        partition_key: b"p",
        id: b"r",
    }];
    for oversized in [false, true] {
        let mut lanes = view.fifo.clone();
        if oversized {
            let lane = LaneKey::for_record(&entry).ok_or("lane")?;
            *lanes
                .get_mut(&lane)
                .ok_or("members")?
                .get_mut(&LaneOrder::for_record(&entry))
                .ok_or("member")? = ClaimRecord {
                record: mirror,
                ..entry
            };
        } else {
            lanes.clear();
        }
        assert_eq!(
            decide_indexed_batch(
                IndexedGroupView {
                    fifo: &lanes,
                    ..view
                },
                &[item],
                order,
                b"entry",
                AdmissionLimits::new(64, 16 * 1024 * 1024, 2)?
            ),
            Err(ApplyError::InvalidAuthority),
            "oversized={oversized}"
        );
    }
    Ok(())
}
