//! Independent summary/tuple and exact scalar boundary controls.
use super::*;

#[test]
fn terminal_closure_matches_independent_wait_policy_tuple_oracle() -> TestResult {
    checks::assert_unresolved_running_parent()?;
    assert_deep_cancellation_order()?;
    let captured = fixtures::lifecycle(MaxActiveOverride::Millis(1), 100, 0, 100)?;
    for wait in [ChildWait::All, ChildWait::Any, ChildWait::None] {
        for failure in [ChildFailure::Ignore, ChildFailure::FailParent] {
            for closed_none in [false, true] {
                if closed_none && wait != ChildWait::None {
                    continue;
                }
                let mut parent = fixture(
                    b"parent",
                    b"p",
                    b"waiting",
                    TimestampMillis::new(0),
                    None,
                    captured,
                )?;
                let group = ChildGroup {
                    wait,
                    on_child_failed: failure,
                    on_parent_closed: ParentClosed::Abandon,
                    success_state: b"ok".to_vec(),
                    failure_state: b"bad".to_vec(),
                    request_hash: Vec::new(),
                    children: [(
                        RecordId::new(b"child".to_vec())?,
                        ChildMember {
                            partition: Some(PartitionId::new(b"different".to_vec())?),
                            status: ChildStatus::Running,
                            result: None,
                        },
                    )]
                    .into(),
                    summary: ChildSummary {
                        total: 1,
                        completed: 0,
                        failed: 0,
                        cancelled: 0,
                    },
                    resolved: closed_none.then_some(ChildResolution::Success),
                };
                parent.child_groups.insert(b"a".to_vec(), group.clone());
                parent.child_groups.insert(b"b".to_vec(), group);
                let mut child = fixture(
                    b"child",
                    b"p",
                    b"queued",
                    TimestampMillis::new(0),
                    Some(TimestampMillis::new(0)),
                    captured,
                )?;
                child.parent = Some(ParentLink {
                    id: b"parent".to_vec(),
                    partition: None,
                });
                let records = [
                    restore_active(parent, AdmissionLimits::default())?,
                    restore_active(child, AdmissionLimits::default())?,
                ];
                with_view(
                    &records,
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    ReleaseOutboxState { head: 1, tail: 0 },
                    |view| {
                        let ActiveTimeoutDecision::Accepted(plan) = expire_at(
                            view,
                            &[ScopedRecordId {
                                partition_key: b"p",
                                id: b"child",
                            }],
                            TimestampMillis::new(1),
                            EntryOrder {
                                identity: LogIdentity::new(1, 7, AppliedPosition::new(3)),
                                first_ordinal: 0,
                            },
                            (AdmissionLimits::default(), TimeoutLimits::default()),
                        )?
                        else {
                            return Err("oracle rejected".into());
                        };
                        checks::assert_child_tuple(&plan, wait, failure, closed_none)?;
                        assert!(plan.deltas().all(|d| d.is_ok()));
                        Ok(())
                    },
                )?;
            }
        }
    }
    Ok(())
}

#[test]
fn timeout_version_summary_ordinal_extremes_are_atomic() -> TestResult {
    assert_ordinal_boundary()?;
    let captured = fixtures::lifecycle(MaxActiveOverride::Millis(1), 31_536_000_000, 0, 1)?;
    let zero = TimestampMillis::new(0);
    for version in [u64::MAX, u64::MAX - 1, 9] {
        let core = restore_record(
            WorkflowRecordData {
                id: RecordId::new(b"r".to_vec())?,
                workflow_type: b"job".to_vec(),
                state: b"queued".to_vec(),
                version,
                fence: Fence::new(7),
                attempts: 8,
                created_at: zero,
                updated_at: TimestampMillis::new(500),
                next_run_at: Some(zero),
                run_state: None,
                lease: None,
                payload: None,
                result: None,
                error: None,
                latest_kind: EventKind::Created,
                latest_version: version,
                latest_fence: Fence::new(7),
            },
            AdmissionLimits::default(),
        )?;
        let mut data = fixture(b"r", b"p", b"queued", zero, Some(zero), captured)?;
        data.core = core;
        let records = [restore_active(data, AdmissionLimits::default())?];
        let history = [(
            (b"p".to_vec(), b"r".to_vec()),
            [(TimestampMillis::new(500), b"500-9".to_vec())].into(),
        )]
        .into();
        with_view(
            &records,
            &BTreeMap::new(),
            &history,
            ReleaseOutboxState { head: 1, tail: 0 },
            |view| {
                let result = expire_at(
                    view,
                    &[ScopedRecordId {
                        partition_key: b"p",
                        id: b"r",
                    }],
                    TimestampMillis::new(1),
                    EntryOrder {
                        identity: LogIdentity::new(1, 7, AppliedPosition::new(u64::MAX)),
                        first_ordinal: u32::MAX,
                    },
                    (AdmissionLimits::default(), TimeoutLimits::default()),
                );
                if version == u64::MAX {
                    return assert_no_plan(
                        result,
                        Ok(ActiveTimeoutRejection::Domain(
                            wf_domain::value::DomainError::CounterOverflow(
                                wf_domain::value::Counter::Version,
                            ),
                        )),
                    );
                }
                let ActiveTimeoutDecision::Accepted(plan) = result? else {
                    return Err("counter rejected".into());
                };
                checks::assert_extreme(&plan, version);
                assert!(plan.deltas().all(|d| d.is_ok()));
                Ok(())
            },
        )?;
    }
    let mut invalid = fixture(b"r", b"p", b"queued", zero, Some(zero), captured)?;
    invalid.child_groups.insert(
        b"g".to_vec(),
        ChildGroup {
            wait: ChildWait::All,
            on_child_failed: ChildFailure::Ignore,
            on_parent_closed: ParentClosed::Abandon,
            success_state: b"ok".to_vec(),
            failure_state: b"bad".to_vec(),
            request_hash: Vec::new(),
            children: BTreeMap::new(),
            summary: ChildSummary {
                total: u64::MAX,
                completed: 1,
                failed: 0,
                cancelled: 0,
            },
            resolved: None,
        },
    );
    assert!(restore_active(invalid, AdmissionLimits::default()).is_err());
    assert!(TimeoutLimits::new(0, 1, 1, 1, 1).is_err());
    assert_history_key_admission()?;
    Ok(())
}

fn assert_history_key_admission() -> TestResult {
    let captured = fixtures::lifecycle(MaxActiveOverride::Millis(500), 100, 0, 1)?;
    let zero = TimestampMillis::new(0);
    let now = TimestampMillis::new(500);
    let small = AdmissionLimits::new(8, 16 * 1_024 * 1_024, 1_000)?;
    let large = AdmissionLimits::new(9, 16 * 1_024 * 1_024, 1_000)?;
    let work = TimeoutLimits::default();
    let records = [restore_active(
        fixture(b"r", b"p", b"queued", zero, Some(zero), captured)?,
        AdmissionLimits::default(),
    )?];
    let history = [(
        (b"p".to_vec(), b"r".to_vec()),
        [(zero, b"opaque-id".to_vec())].into(),
    )]
    .into();
    let originals = records.clone();
    with_view(
        &records,
        &BTreeMap::new(),
        &history,
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let root = ScopedRecordId {
                partition_key: b"p",
                id: b"r",
            };
            let order = EntryOrder {
                identity: LogIdentity::new(1, 7, AppliedPosition::new(2)),
                first_ordinal: 0,
            };
            let actual = expire_at(view, &[root], now, order, (small, work));
            assert!(
                !matches!(actual, Ok(ActiveTimeoutDecision::Accepted(_))),
                "oversized old key exposed Accepted"
            );
            assert_no_plan(actual, Err(ApplyError::InvalidAuthority))?;
            let ActiveTimeoutDecision::Accepted(plan) =
                expire_at(view, &[root], now, order, (large, work))?
            else {
                return Err("admitted historical key rejected".into());
            };
            assert_eq!(plan.trims().len(), 1);
            assert_eq!(plan.trims()[0].original.len(), 1);
            let old = view.history[&root].first().ok_or("old key")?;
            assert!(plan.trims()[0].original[0] == *old);
            assert!(std::ptr::eq(
                plan.trims()[0].original[0].event_id,
                old.event_id
            ));
            assert_eq!(plan.history().len(), 1);
            assert_eq!(plan.history()[0].kind, EventKind::Failed);
            Ok(())
        },
    )?;
    assert!(records == originals);
    Ok(())
}

fn assert_ordinal_boundary() -> TestResult {
    let finite = fixtures::lifecycle(MaxActiveOverride::Millis(1), 100, 0, 10)?;
    let infinite = fixtures::lifecycle(MaxActiveOverride::Infinite, 100, 0, 10)?;
    let mut records = Vec::new();
    for partition in [b"p".as_slice(), b"q"] {
        let mut data = fixtures::family_data(finite, infinite, b"waiting", (partition, partition))?;
        data[0]
            .child_groups
            .get_mut(b"g".as_slice())
            .ok_or("group")?
            .on_parent_closed = ParentClosed::Abandon;
        for data in data {
            records.push(restore_active(data, AdmissionLimits::default())?);
        }
    }
    let originals = records.clone();
    let roots = [
        ScopedRecordId {
            partition_key: b"p",
            id: b"child",
        },
        ScopedRecordId {
            partition_key: b"q",
            id: b"child",
        },
    ];
    let order = EntryOrder {
        identity: LogIdentity::new(1, 7, AppliedPosition::new(u64::MAX)),
        first_ordinal: u32::MAX,
    };
    let profiles = (AdmissionLimits::default(), TimeoutLimits::default());
    with_view(
        &records,
        &BTreeMap::new(),
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let ActiveTimeoutDecision::Accepted(plan) =
                expire_at(view, &roots[..1], TimestampMillis::new(1), order, profiles)?
            else {
                return Err("last ordinal rejected".into());
            };
            let parent = plan
                .record(ScopedRecordId {
                    partition_key: b"p",
                    id: b"parent",
                })
                .ok_or("parent")?;
            assert_eq!(parent.ordering().state_enter_seq, (1_u128 << 96) - 1);
            assert_eq!(plan.consumed_ordinals(), 1);
            assert_eq!(plan.next_ordinal(), None);
            assert_eq!((plan.writes().len(), plan.history().len()), (2, 2));
            assert!(plan.writes().iter().all(|w| w.expected_version == 1));
            assert_no_plan(
                expire_at(view, &roots, TimestampMillis::new(1), order, profiles),
                Err(ApplyError::InvalidAuthority),
            )
        },
    )?;
    assert!(records == originals);
    Ok(())
}

fn assert_deep_cancellation_order() -> TestResult {
    let finite = fixtures::lifecycle(MaxActiveOverride::Millis(1), 100, 0, 100)?;
    let infinite = fixtures::lifecycle(MaxActiveOverride::Infinite, 100, 0, 100)?;
    let template = fixtures::overlay_records(finite)?[0].child_groups()[b"g".as_slice()].clone();
    assert!(template.resolved == Some(ChildResolution::Success));
    assert!(template.wait == ChildWait::None);
    let ids = [b"a".as_slice(), b"b", b"c", b"d", b"e", b"f"];
    let zero = TimestampMillis::new(0);
    let mut records = Vec::new();
    for (position, id) in ids.iter().enumerate() {
        let captured = if position == 0 { finite } else { infinite };
        let mut data = fixture(id, b"p", b"queued", zero, Some(zero), captured)?;
        if position > 0 {
            data.parent = Some(ParentLink {
                id: ids[position - 1].to_vec(),
                partition: None,
            });
        }
        if position + 1 < ids.len() {
            let mut group = template.clone();
            let member = group
                .children
                .values()
                .next()
                .ok_or("template member")?
                .clone();
            group.children = [(RecordId::new(ids[position + 1].to_vec())?, member)].into();
            data.child_groups.insert(b"g".to_vec(), group);
        }
        records.push(restore_active(data, AdmissionLimits::default())?);
    }
    let originals = records.clone();
    with_view(
        &records,
        &BTreeMap::new(),
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let ActiveTimeoutDecision::Accepted(plan) = expire_at(
                view,
                &[ScopedRecordId {
                    partition_key: b"p",
                    id: b"a",
                }],
                TimestampMillis::new(1),
                EntryOrder {
                    identity: LogIdentity::new(1, 7, AppliedPosition::new(2)),
                    first_ordinal: 0,
                },
                (AdmissionLimits::default(), TimeoutLimits::default()),
            )?
            else {
                return Err("deep closure rejected".into());
            };
            assert_eq!(plan.writes().len(), ids.len());
            for (position, (write, id)) in plan.writes().iter().zip(ids).enumerate() {
                assert_eq!(write.record.core().id().as_bytes(), id);
                assert_eq!(write.expected_version, 1);
                assert_eq!(
                    write.record.core().version(),
                    if position == 5 { 2 } else { 3 }
                );
            }
            assert_eq!(plan.history().len(), 11);
            let expected = [
                (EventKind::Failed, b"a".as_slice()),
                (EventKind::Cancelled, b"b"),
                (EventKind::Cancelled, b"c"),
                (EventKind::Cancelled, b"d"),
                (EventKind::Cancelled, b"e"),
                (EventKind::Cancelled, b"f"),
                (EventKind::ChildrenCancelled, b"e"),
                (EventKind::ChildrenCancelled, b"d"),
                (EventKind::ChildrenCancelled, b"c"),
                (EventKind::ChildrenCancelled, b"b"),
                (EventKind::ChildrenCancelled, b"a"),
            ];
            assert_eq!(
                plan.history()
                    .iter()
                    .map(|h| (h.kind, h.id.as_bytes()))
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(plan.stats(), 1);
            assert_eq!(plan.consumed_ordinals(), 0);
            assert!(plan.deltas().all(|d| d.is_ok()));
            Ok(())
        },
    )?;
    assert!(records == originals);
    Ok(())
}
