//! Extracted assertion groups; expectations remain independent of reducer output.
use super::*;
use wf_domain::workflow::admission::AdmissionError;

pub(super) fn assert_timeout_record(plan: &ActiveTimeoutPlan<'_>) -> TestResult {
    let write = &plan.writes()[0];
    assert_eq!(write.expected_version, 1);
    assert_eq!(
        (
            write.record.core().state(),
            write.record.core().version(),
            write.record.core().fence().get()
        ),
        (b"failed".as_slice(), 2, 1)
    );
    assert_eq!(write.record.core().attempts(), 0);
    assert!(write.record.core().next_run_at().is_none() && write.record.core().lease().is_none());
    assert!(write.record.ttl_ms().is_none() && write.record.core().error().is_none());
    assert_eq!(
        write.record.terminal_retention_until(),
        Some(TimestampMillis::new(1_500))
    );
    assert_eq!(
        write.record.carry().attributes_history.as_deref(),
        Some(&[][..])
    );
    assert!(write.record.values().payload == Some(ActiveValueRef::Opaque(b"payload-ref".to_vec())));
    assert!(
        write.record.values().error
            == Some(ActiveValueRef::Versioned {
                partition: PartitionId::new(b"p".to_vec())?,
                id: RecordId::new(b"r".to_vec())?,
                kind: wf_domain::workflow::active::ActiveValueKind::Error,
                version: 2
            })
    );
    assert_eq!(plan.value_writes()[0].body.max_active_ms, 500);
    assert_eq!(
        plan.value_writes()[0].expires_at,
        Some(TimestampMillis::new(1_500))
    );
    Ok(())
}

pub(super) fn assert_timeout_indexes(
    plan: &ActiveTimeoutPlan<'_>,
    root: ScopedRecordId<'_>,
) -> TestResult {
    assert_history(
        plan,
        &[(EventKind::Failed, 2, b"failed", TimestampMillis::new(500))],
    );
    assert_eq!(plan.history()[0].event_id, "500-2");
    assert_eq!(
        plan.history()[0].timeout.map(|e| e.max_active_ms),
        Some(500)
    );
    assert_eq!(plan.consumed_ordinals(), 0);
    assert_eq!(plan.next_ordinal(), Some(0));
    assert_eq!(plan.hot_evictions().count(), 1);
    let deltas: Vec<_> = plan.deltas().collect::<Result<_, _>>()?;
    assert_eq!(deltas.len(), 1);
    assert!(
        deltas[0]
            .after
            .as_ref()
            .is_some_and(|rows| rows.parallel.is_none() && rows.fifo.is_none())
    );
    assert!(
        deltas[0]
            .after
            .as_ref()
            .ok_or("missing rows")?
            .semantic
            .iter()
            .flatten()
            .any(
                |(key, score)| matches!(key, ActiveIndexKey::TerminalRetention(id) if *id == root)
                    && *score == 1_500
            )
    );
    Ok(())
}

pub(super) fn assert_child_closure(
    plan: &ActiveTimeoutPlan<'_>,
    now: TimestampMillis,
    index: u64,
) -> TestResult {
    assert_eq!(plan.stats(), 1);
    assert_eq!(plan.writes().len(), 3);
    assert_history(
        plan,
        &[
            (EventKind::Failed, 2, b"failed", now),
            (EventKind::ChildFailed, 2, b"children_failed", now),
            (EventKind::Cancelled, 2, b"cancelled", now),
            (EventKind::ChildrenCancelled, 3, b"children_failed", now),
        ],
    );
    let parent = plan
        .record(ScopedRecordId {
            partition_key: b"a",
            id: b"parent",
        })
        .ok_or("missing parent")?;
    assert_eq!(parent.core().version(), 3);
    assert_eq!(parent.core().status(), WorkflowStatus::Unscheduled);
    assert!(
        parent.child_groups()[b"g".as_slice()].summary
            == ChildSummary {
                total: 2,
                completed: 0,
                failed: 1,
                cancelled: 1
            }
    );
    assert!(parent.child_groups()[b"g".as_slice()].resolved == Some(ChildResolution::Failure));
    assert_eq!(parent.ordering().state_enter_seq, u128::from(index) << 32);
    assert_eq!(plan.consumed_ordinals(), 1);
    assert_eq!(plan.next_ordinal(), Some(1));
    assert_eq!(plan.writes()[1].expected_version, 1);
    assert_eq!(plan.writes()[2].record.core().id().as_bytes(), b"sibling");
    assert_eq!(
        plan.history()
            .iter()
            .filter(|h| h.kind == EventKind::ChildCancelled)
            .count(),
        0
    );
    assert!(plan.deltas().all(|delta| delta.is_ok()));
    Ok(())
}

pub(super) fn assert_corrupt_view(
    view: ActiveTimeoutView<'_>,
    root: ScopedRecordId<'_>,
    order: EntryOrder,
) -> TestResult {
    let mut placements = view.indexed.batch.placements.clone();
    placements.insert(
        PartitionId::new(b"p".to_vec())?,
        GroupId::new(b"other".to_vec())?,
    );
    let foreign = ActiveTimeoutView {
        indexed: IndexedGroupView {
            batch: GroupView {
                placements: &placements,
                ..view.indexed.batch
            },
            ..view.indexed
        },
        ..view
    };
    assert_no_plan(
        expire_at(
            foreign,
            &[root],
            TimestampMillis::new(1),
            order,
            (AdmissionLimits::default(), TimeoutLimits::default()),
        ),
        Ok(ActiveTimeoutRejection::CrossGroup),
    )?;
    let mut lifecycle = view.lifecycle.clone();
    lifecycle.remove(&ActiveIndexKey::DueAny {
        workflow_type: b"job",
        priority: 0,
        record: root,
    });
    assert_no_plan(
        expire_at(
            ActiveTimeoutView {
                lifecycle: &lifecycle,
                ..view
            },
            &[root],
            TimestampMillis::new(1),
            order,
            (AdmissionLimits::default(), TimeoutLimits::default()),
        ),
        Err(ApplyError::InvalidAuthority),
    )?;
    let cores = BTreeMap::new();
    assert_no_plan(
        expire_at(
            ActiveTimeoutView {
                indexed: IndexedGroupView {
                    batch: GroupView {
                        records: &cores,
                        ..view.indexed.batch
                    },
                    ..view.indexed
                },
                ..view
            },
            &[root],
            TimestampMillis::new(1),
            order,
            (AdmissionLimits::default(), TimeoutLimits::default()),
        ),
        Err(ApplyError::InvalidAuthority),
    )?;
    assert_eq!(view.records[&root].core().state(), b"queued");
    Ok(())
}

pub(super) fn assert_budget(
    view: ActiveTimeoutView<'_>,
    root: ScopedRecordId<'_>,
    order: EntryOrder,
    p: u64,
    g: u64,
) -> TestResult {
    let exact = TimeoutLimits::new(100_000, 100_000, 100_000, 100_000, p + g)?;
    let ActiveTimeoutDecision::Accepted(exact_plan) = expire_at(
        view,
        &[root],
        TimestampMillis::new(500),
        order,
        (
            AdmissionLimits::new(1_048_576, usize::try_from(p)?, 1_000)?,
            exact,
        ),
    )?
    else {
        return Err("exact budget rejected".into());
    };
    assert_eq!(exact_plan.charges(), (p, g));
    assert_no_plan(
        expire_at(
            view,
            &[root],
            TimestampMillis::new(500),
            order,
            (
                AdmissionLimits::new(1_048_576, usize::try_from(p - 1)?, 1_000)?,
                TimeoutLimits::default(),
            ),
        ),
        Ok(ActiveTimeoutRejection::Admission(
            AdmissionError::DecisionBytes,
        )),
    )?;
    assert_no_plan(
        expire_at(
            view,
            &[root],
            TimestampMillis::new(500),
            order,
            (
                AdmissionLimits::default(),
                TimeoutLimits::new(100_000, 100_000, 100_000, 100_000, p + g - 1)?,
            ),
        ),
        Ok(ActiveTimeoutRejection::WorkBytes {
            bytes: p + g,
            maximum: p + g - 1,
        }),
    )?;
    assert_no_plan(
        expire_at(
            view,
            &[root, root],
            TimestampMillis::new(500),
            order,
            (
                AdmissionLimits::new(1_048_576, 16_777_216, 1)?,
                TimeoutLimits::default(),
            ),
        ),
        Ok(ActiveTimeoutRejection::Admission(
            AdmissionError::BatchItems,
        )),
    )?;
    assert_eq!(view.records[&root].core().version(), 1);
    Ok(())
}

pub(super) fn assert_child_tuple(
    plan: &ActiveTimeoutPlan<'_>,
    wait: ChildWait,
    failure: ChildFailure,
    closed_none: bool,
) -> TestResult {
    let expected_state: &[u8] = if closed_none {
        b"waiting"
    } else if failure == ChildFailure::FailParent || wait == ChildWait::Any {
        b"bad"
    } else {
        b"ok"
    };
    let expected = (
        if closed_none { 1 } else { 2 },
        expected_state,
        u64::from(!closed_none),
        if closed_none { 1 } else { 2 },
    );
    let parent = plan
        .record(ScopedRecordId {
            partition_key: b"p",
            id: b"parent",
        })
        .ok_or("parent")?;
    assert_eq!(
        (
            parent.core().version(),
            parent.core().state(),
            parent.child_groups()[b"a".as_slice()].summary.failed,
            plan.history().len()
        ),
        expected
    );
    assert_eq!(parent.child_groups()[b"b".as_slice()].summary.failed, 0);
    assert!(
        parent.child_groups()[b"b".as_slice()].children[&RecordId::new(b"child".to_vec())?].status
            == ChildStatus::Running
    );
    if !closed_none {
        assert!(
            parent.child_groups()[b"a".as_slice()].children[&RecordId::new(b"child".to_vec())?]
                .result
                .as_ref()
                .ok_or("child result")?
                .error_ref
                == plan.writes()[0].record.values().error
        );
    }
    assert_eq!(plan.writes().len(), if closed_none { 1 } else { 2 });
    Ok(())
}

pub(super) fn assert_extreme(plan: &ActiveTimeoutPlan<'_>, version: u64) {
    assert_eq!(plan.writes()[0].record.core().version(), version + 1);
    assert_eq!(
        (
            plan.writes()[0].record.core().fence().get(),
            plan.writes()[0].record.core().attempts()
        ),
        (7, 8)
    );
    assert_eq!(plan.consumed_ordinals(), 0);
    assert_eq!(plan.next_ordinal(), Some(u32::MAX));
    if version == 9 {
        assert_eq!(plan.history()[0].event_id, "500-10");
        assert_eq!(plan.history()[0].occurred_at, TimestampMillis::new(1));
        assert_eq!(plan.trims().len(), 1);
        assert_eq!(plan.trims()[0].created, [0]);
        assert!(plan.trims()[0].original.is_empty());
    }
}

pub(super) fn assert_unresolved_running_parent() -> TestResult {
    let child_lifecycle = fixtures::lifecycle(MaxActiveOverride::Millis(1), 100, 0, 10)?;
    let parent_lifecycle = fixtures::lifecycle(MaxActiveOverride::Infinite, 100, 0, 10)?;
    let mut data =
        fixtures::family_data(child_lifecycle, parent_lifecycle, b"running", (b"p", b"p"))?;
    let group = data[0]
        .child_groups
        .get_mut(b"g".as_slice())
        .ok_or("group")?;
    group.on_child_failed = ChildFailure::Ignore;
    group.on_parent_closed = ParentClosed::Abandon;
    let records = data
        .into_iter()
        .map(|data| Ok(restore_active(data, AdmissionLimits::default())?))
        .collect::<TestResult<Vec<_>>>()?;
    let originals = records.clone();
    with_view(
        &records,
        &BTreeMap::new(),
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let child = ScopedRecordId {
                partition_key: b"p",
                id: b"child",
            };
            let parent = ScopedRecordId {
                partition_key: b"p",
                id: b"parent",
            };
            let sibling = ScopedRecordId {
                partition_key: b"p",
                id: b"sibling",
            };
            let ActiveTimeoutDecision::Accepted(plan) = expire_at(
                view,
                &[child],
                TimestampMillis::new(1),
                EntryOrder {
                    identity: LogIdentity::new(1, 7, AppliedPosition::new(2)),
                    first_ordinal: 0,
                },
                (AdmissionLimits::default(), TimeoutLimits::default()),
            )?
            else {
                return Err("unresolved parent rejected".into());
            };
            assert_eq!((plan.writes().len(), plan.history().len()), (2, 2));
            assert_history(
                &plan,
                &[
                    (EventKind::Failed, 2, b"failed", TimestampMillis::new(1)),
                    (
                        EventKind::ChildFailed,
                        2,
                        b"running",
                        TimestampMillis::new(1),
                    ),
                ],
            );
            let after = plan.record(parent).ok_or("parent")?;
            assert_eq!(
                (after.core().state(), after.core().version()),
                (b"running".as_slice(), 2)
            );
            assert_eq!(after.core().lease(), originals[0].core().lease());
            assert_eq!(after.core().next_run_at(), Some(TimestampMillis::new(50)));
            assert_eq!(
                (
                    after.child_groups()[b"g".as_slice()].summary.total,
                    after.child_groups()[b"g".as_slice()].summary.failed
                ),
                (2, 1)
            );
            assert!(after.child_groups()[b"g".as_slice()].resolved.is_none());
            assert!(plan.record(sibling).ok_or("sibling")? == &originals[2]);
            assert_eq!(
                after.ordering().state_enter_seq,
                originals[0].ordering().state_enter_seq
            );
            assert_eq!(plan.consumed_ordinals(), 0);
            assert_eq!(plan.next_ordinal(), Some(0));
            assert!(records == originals);
            Ok(())
        },
    )
}
