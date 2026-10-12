//! Exact timeout and recursive same-group effects.
use super::*;

mod fifo;

#[test]
fn timeout_writes_exact_error_history_retention_and_index_effects() -> TestResult {
    let captured = fixtures::lifecycle(MaxActiveOverride::Millis(500), 1_000, 0, 100)?;
    let mut data = fixture(
        b"r",
        b"p",
        b"queued",
        TimestampMillis::new(0),
        Some(TimestampMillis::new(0)),
        captured,
    )?;
    data.carry.attributes_history = Some(Vec::new());
    data.values.payload = Some(ActiveValueRef::Opaque(b"payload-ref".to_vec()));
    let record = restore_active(data, AdmissionLimits::default())?;
    with_view(
        &[record],
        &BTreeMap::new(),
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let root = ScopedRecordId {
                partition_key: b"p",
                id: b"r",
            };
            let ActiveTimeoutDecision::Accepted(plan) = expire_at(
                view,
                &[root],
                TimestampMillis::new(500),
                EntryOrder {
                    identity: LogIdentity::new(1, 7, AppliedPosition::new(11)),
                    first_ordinal: 0,
                },
                (AdmissionLimits::default(), TimeoutLimits::default()),
            )?
            else {
                return Err("timeout rejected".into());
            };
            assert_eq!(
                (plan.stats(), plan.writes().len(), plan.value_writes().len()),
                (1, 1, 1)
            );
            checks::assert_timeout_record(&plan)?;
            checks::assert_timeout_indexes(&plan, root)?;
            assert_eq!(view.records[&root].core().state(), b"queued");
            Ok(())
        },
    )
}

#[test]
fn child_timeout_resolves_cross_partition_parent_and_cancels_siblings() -> TestResult {
    let captured = fixtures::lifecycle(MaxActiveOverride::Millis(500), 1_000, 0, 100)?;
    let forever = fixtures::lifecycle(MaxActiveOverride::Infinite, 1_000, 0, 100)?;
    let records = fixtures::family_data(captured, forever, b"waiting", (b"a", b"b"))?
        .into_iter()
        .map(|data| Ok(restore_active(data, AdmissionLimits::default())?))
        .collect::<TestResult<Vec<_>>>()?;
    with_view(
        &records,
        &BTreeMap::new(),
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let ActiveTimeoutDecision::Accepted(plan) = expire_at(
                view,
                &[ScopedRecordId {
                    partition_key: b"b",
                    id: b"child",
                }],
                TimestampMillis::new(500),
                EntryOrder {
                    identity: LogIdentity::new(1, 7, AppliedPosition::new(11)),
                    first_ordinal: 0,
                },
                (AdmissionLimits::default(), TimeoutLimits::default()),
            )?
            else {
                return Err("closure rejected".into());
            };
            checks::assert_child_closure(&plan, TimestampMillis::new(500), 11)?;
            Ok(())
        },
    )
}

#[test]
fn later_roots_refetch_parent_child_overlay_without_double_counting() -> TestResult {
    let captured = fixtures::lifecycle(MaxActiveOverride::Millis(1), 100, 1, 2)?;
    let records = fixtures::overlay_records(captured)?;
    let writes = with_view(
        &records,
        &BTreeMap::new(),
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let parent = ScopedRecordId {
                partition_key: b"p",
                id: b"parent",
            };
            let child = ScopedRecordId {
                partition_key: b"p",
                id: b"child",
            };
            let ActiveTimeoutDecision::Accepted(plan) = expire_at(
                view,
                &[parent, child, parent],
                TimestampMillis::new(1),
                EntryOrder {
                    identity: LogIdentity::new(1, 7, AppliedPosition::new(9)),
                    first_ordinal: 0,
                },
                (AdmissionLimits::default(), TimeoutLimits::default()),
            )?
            else {
                return Err("overlay rejected".into());
            };
            assert_eq!(
                (plan.stats(), plan.writes().len(), plan.history().len()),
                (1, 2, 3)
            );
            assert_history(
                &plan,
                &[
                    (EventKind::Failed, 2, b"failed", TimestampMillis::new(1)),
                    (
                        EventKind::Cancelled,
                        2,
                        b"cancelled",
                        TimestampMillis::new(1),
                    ),
                    (
                        EventKind::ChildrenCancelled,
                        3,
                        b"failed",
                        TimestampMillis::new(1),
                    ),
                ],
            );
            assert_eq!(
                plan.record(parent).ok_or("parent")?.child_groups()[b"g".as_slice()]
                    .summary
                    .cancelled,
                1
            );
            assert_eq!(plan.value_writes().len(), 1);
            assert!(plan.deltas().all(|delta| delta.is_ok()));
            Ok(plan.writes().to_vec())
        },
    )?;
    let records = apply_timeout_plan(records, writes)?;
    assert_eq!(records[0].core().version(), 3);
    assert_eq!(records[1].core().state(), b"cancelled");
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
                    id: b"parent",
                }],
                TimestampMillis::new(2),
                EntryOrder {
                    identity: LogIdentity::new(1, 7, AppliedPosition::new(10)),
                    first_ordinal: 0,
                },
                (AdmissionLimits::default(), TimeoutLimits::default()),
            )?
            else {
                return Err("terminal replay rejected".into());
            };
            assert_eq!(plan.stats(), 0);
            assert!(plan.writes().is_empty());
            Ok(())
        },
    )
}
