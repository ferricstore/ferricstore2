//! Authority failures and deterministic pre-copy resource rejection.
use super::*;

#[test]
fn cross_group_and_corrupt_authority_expose_no_partial_plan() -> TestResult {
    let captured = fixtures::lifecycle(MaxActiveOverride::Millis(1), 100, 0, 10)?;
    let records = [restore_active(
        fixture(
            b"r",
            b"p",
            b"queued",
            TimestampMillis::new(0),
            Some(TimestampMillis::new(0)),
            captured,
        )?,
        AdmissionLimits::default(),
    )?];
    with_view(
        &records,
        &BTreeMap::new(),
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let root = ScopedRecordId {
                partition_key: b"p",
                id: b"r",
            };
            let order = EntryOrder {
                identity: LogIdentity::new(1, 7, AppliedPosition::new(1)),
                first_ordinal: 0,
            };
            checks::assert_corrupt_view(view, root, order)?;
            Ok(())
        },
    )?;
    let unowned = restore_active(
        fixture(
            b"nil",
            b"p",
            b"running",
            TimestampMillis::new(0),
            None,
            captured,
        )?,
        AdmissionLimits::default(),
    )?;
    with_view(
        &[unowned],
        &BTreeMap::new(),
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            assert_no_plan(
                expire_at(
                    view,
                    &[ScopedRecordId {
                        partition_key: b"p",
                        id: b"nil",
                    }],
                    TimestampMillis::new(1),
                    EntryOrder {
                        identity: LogIdentity::new(1, 7, AppliedPosition::new(1)),
                        first_ordinal: 0,
                    },
                    (AdmissionLimits::default(), TimeoutLimits::default()),
                ),
                Err(ApplyError::InvalidAuthority),
            )
        },
    )?;
    assert_lineage_authority_paths()
}

fn assert_lineage_authority_paths() -> TestResult {
    let finite = fixtures::lifecycle(MaxActiveOverride::Millis(1), 100, 0, 10)?;
    let infinite = fixtures::lifecycle(MaxActiveOverride::Infinite, 100, 0, 10)?;
    let zero = TimestampMillis::new(0);
    let order = EntryOrder {
        identity: LogIdentity::new(1, 7, AppliedPosition::new(2)),
        first_ordinal: 0,
    };
    let profiles = (AdmissionLimits::default(), TimeoutLimits::default());
    for scenario in 0..3 {
        let parent_partition: &[u8] = if scenario == 1 { b"q" } else { b"p" };
        let mut data =
            fixtures::family_data(finite, infinite, b"waiting", (parent_partition, b"p"))?;
        let group = data[0]
            .child_groups
            .get_mut(b"g".as_slice())
            .ok_or("group")?;
        if scenario == 0 {
            group.on_parent_closed = ParentClosed::Abandon;
            let member = group.children.values().next().ok_or("member")?.clone();
            group.children = [(RecordId::new(b"parent".to_vec())?, member)].into();
            group.summary.total = 1;
            data[0].core = fixture(b"parent", b"p", b"queued", zero, Some(zero), finite)?.core;
            data[0].captured = finite;
            data[0].parent = Some(ParentLink {
                id: b"parent".to_vec(),
                partition: None,
            });
            data.truncate(1);
        } else if scenario == 2 {
            group.failure_state = b"running".to_vec();
        }
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
                let mut placements = view.indexed.batch.placements.clone();
                if scenario == 1 {
                    // Only the backlink dependency's canonical placement changes.
                    placements.insert(
                        PartitionId::new(b"q".to_vec())?,
                        GroupId::new(b"other".to_vec())?,
                    );
                }
                let view = ActiveTimeoutView {
                    indexed: IndexedGroupView {
                        batch: GroupView {
                            placements: &placements,
                            ..view.indexed.batch
                        },
                        ..view.indexed
                    },
                    ..view
                };
                let root = ScopedRecordId {
                    partition_key: b"p",
                    id: if scenario == 0 { b"parent" } else { b"child" },
                };
                let actual = expire_at(view, &[root], TimestampMillis::new(1), order, profiles);
                assert!(
                    !matches!(actual, Ok(ActiveTimeoutDecision::Accepted(_))),
                    "active cycle exposed Accepted"
                );
                assert_no_plan(actual, Err(ApplyError::InvalidAuthority))
            },
        )?;
        assert!(records == originals);
    }
    Ok(())
}

#[test]
fn closure_budget_exact_boundaries_precede_owned_copies() -> TestResult {
    let captured = fixtures::lifecycle(MaxActiveOverride::Millis(500), 1_000, 0, 100)?;
    let records = [restore_active(
        fixture(
            b"r",
            b"p",
            b"queued",
            TimestampMillis::new(0),
            Some(TimestampMillis::new(0)),
            captured,
        )?,
        AdmissionLimits::default(),
    )?];
    with_view(
        &records,
        &BTreeMap::new(),
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let root = ScopedRecordId {
                partition_key: b"p",
                id: b"r",
            };
            let order = EntryOrder {
                identity: LogIdentity::new(1, 7, AppliedPosition::new(11)),
                first_ordinal: 0,
            };
            let ActiveTimeoutDecision::Accepted(plan) = expire_at(
                view,
                &[root],
                TimestampMillis::new(500),
                order,
                (AdmissionLimits::default(), TimeoutLimits::default()),
            )?
            else {
                return Err("baseline rejected".into());
            };
            // R=11, replacement=8, history=18, value identity=2, five fixed descriptors.
            assert_eq!(plan.charges().0, 11 + 8 + 18 + 2 + 5 * 1_024);
            let (p, g) = plan.charges();
            checks::assert_budget(view, root, order, p, g)?;
            Ok(())
        },
    )?;
    assert_discovery_limits()?;
    assert_link_limits()
}

#[test]
fn release_intents_preserve_source_write_order_and_capacity() -> TestResult {
    let captured = fixtures::lifecycle(MaxActiveOverride::Millis(1), 100, 0, 10)?;
    let mut data = fixtures::family_data(captured, captured, b"running", (b"p", b"p"))?;
    data.swap(0, 1);
    let mut records = Vec::new();
    for mut data in data {
        let id = data.core.id().as_bytes();
        data.core = fixture(
            id,
            b"p",
            b"running",
            TimestampMillis::new(0),
            Some(TimestampMillis::new(10)),
            captured,
        )?
        .core;
        data.governance = Some(GovernanceReservation {
            scope: b"scope".to_vec(),
            shard_id: 0,
            reservation_id: data.core.id().as_bytes().to_vec(),
            enforcement: LimitEnforcement::StrictGlobal,
        });
        if let Some(group) = data.child_groups.get_mut(b"g".as_slice()) {
            group.success_state = b"done".to_vec();
            group.failure_state = b"bad".to_vec();
            group.request_hash = Vec::new();
        }
        records.push(restore_active(data, AdmissionLimits::default())?);
    }
    for outbox in [
        ReleaseOutboxState { head: 1, tail: 0 },
        ReleaseOutboxState {
            head: 1,
            tail: 65_533,
        },
        ReleaseOutboxState {
            head: MAX_EXACT_MILLIS + 1,
            tail: MAX_EXACT_MILLIS,
        },
    ] {
        let empty = outbox.tail == MAX_EXACT_MILLIS;
        let loaded = if empty { &records[..0] } else { &records };
        let roots = [ScopedRecordId {
            partition_key: b"p",
            id: b"child",
        }];
        with_view(loaded, &BTreeMap::new(), &BTreeMap::new(), outbox, |view| {
            if !empty {
                assert_release_extrema(view)?;
            }
            let result = expire_at(
                view,
                if empty { &roots[..0] } else { &roots },
                TimestampMillis::new(1),
                EntryOrder {
                    identity: LogIdentity::new(1, 7, AppliedPosition::new(2)),
                    first_ordinal: 0,
                },
                (AdmissionLimits::default(), TimeoutLimits::default()),
            );
            if outbox.tail != 0 && !empty {
                return assert_no_plan(result, Ok(ActiveTimeoutRejection::ReleaseBacklog));
            }
            let ActiveTimeoutDecision::Accepted(plan) = result? else {
                return Err("release rejected".into());
            };
            if empty {
                assert!(plan.releases().is_empty());
                assert!(plan.outbox().0 == plan.outbox().1);
                return Ok(());
            }
            assert_eq!(plan.releases().len(), 4);
            assert_eq!(
                plan.releases()
                    .iter()
                    .map(|r| r.intent.reservation_id.as_slice())
                    .collect::<Vec<_>>(),
                [b"child".as_slice(), b"parent", b"sibling", b"parent"]
            );
            assert_eq!(
                plan.releases()
                    .iter()
                    .map(|r| r.sequence)
                    .collect::<Vec<_>>(),
                [1, 2, 3, 4]
            );
            assert!(plan.outbox() == (outbox, ReleaseOutboxState { head: 1, tail: 4 }));
            let writes = plan.writes();
            let stripped = writes.iter().all(|w| w.record.governance().is_none());
            assert!(stripped);
            assert_eq!(
                ActiveTimeoutRejection::ReleaseBacklog.to_string(),
                "ERR flow governance release outbox backlog is full"
            );
            Ok(())
        })?;
    }
    Ok(())
}

fn assert_scalar_budget<'v>(
    view: ActiveTimeoutView<'v>,
    roots: &[ScopedRecordId<'_>],
    exact: TimeoutLimits,
    reduced: TimeoutLimits,
    expected: ActiveTimeoutRejection,
) -> TestResult<Box<ActiveTimeoutPlan<'v>>> {
    let order = EntryOrder {
        identity: LogIdentity::new(1, 7, AppliedPosition::new(11)),
        first_ordinal: 0,
    };
    let now = TimestampMillis::new(500);
    let admission = AdmissionLimits::default();
    let ActiveTimeoutDecision::Accepted(plan) =
        expire_at(view, roots, now, order, (admission, exact))?
    else {
        return Err("exact scalar profile rejected".into());
    };
    assert_no_plan(
        expire_at(view, roots, now, order, (admission, reduced)),
        Ok(expected),
    )?;
    Ok(plan)
}

fn assert_discovery_limits() -> TestResult {
    let zero = TimestampMillis::new(0);
    let captured = fixtures::lifecycle(MaxActiveOverride::Millis(500), 1_000, 0, 100)?;
    let mut records = Vec::new();
    for id in [b"r".as_slice(), b"s"] {
        records.push(restore_active(
            fixture(id, b"p", b"queued", zero, Some(zero), captured)?,
            AdmissionLimits::default(),
        )?);
    }
    let roots = [
        ScopedRecordId {
            partition_key: b"p",
            id: b"r",
        },
        ScopedRecordId {
            partition_key: b"p",
            id: b"s",
        },
    ];
    let originals = records.clone();
    with_view(
        &records,
        &BTreeMap::new(),
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            // Independent occurrence enumeration: 32 visits, 163 names, 42 Eq bytes, one step charge.
            let plan = assert_scalar_budget(
                view,
                &roots[..1],
                TimeoutLimits::new(32, 100, 100, 100, 128 << 20)?,
                TimeoutLimits::new(31, 100, 100, 100, 128 << 20)?,
                ActiveTimeoutRejection::Visits {
                    count: 32,
                    maximum: 31,
                },
            )?;
            assert_eq!(plan.charges(), (5_159, 33_997));
            assert_eq!(plan.writes().len(), 1);
            let plan = assert_scalar_budget(
                view,
                &roots,
                TimeoutLimits::new(100_000, 2, 100, 100, 128 << 20)?,
                TimeoutLimits::new(100_000, 1, 100, 100, 128 << 20)?,
                ActiveTimeoutRejection::Steps {
                    count: 2,
                    maximum: 1,
                },
            )?;
            assert_eq!((plan.stats(), plan.writes().len()), (2, 2));
            Ok(())
        },
    )?;
    assert!(records == originals);
    let capped = fixtures::lifecycle(MaxActiveOverride::Millis(500), 1_000, 0, 1)?;
    let records = [restore_active(
        fixture(b"r", b"p", b"queued", zero, Some(zero), capped)?,
        AdmissionLimits::default(),
    )?];
    let history = [(
        (b"p".to_vec(), b"r".to_vec()),
        [
            (zero, b"old-a".to_vec()),
            (TimestampMillis::new(1), b"old-b".to_vec()),
        ]
        .into(),
    )]
    .into();
    with_view(
        &records,
        &BTreeMap::new(),
        &history,
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let plan = assert_scalar_budget(
                view,
                &roots[..1],
                TimeoutLimits::new(100_000, 100, 100, 2, 128 << 20)?,
                TimeoutLimits::new(100_000, 100, 100, 1, 128 << 20)?,
                ActiveTimeoutRejection::HistoryDeletes {
                    count: 2,
                    maximum: 1,
                },
            )?;
            assert_eq!(plan.trims().len(), 1);
            assert_eq!(plan.trims()[0].original.len(), 2);
            assert_eq!(plan.trims()[0].created, [] as [usize; 0]);
            assert!(
                plan.trims()[0]
                    .original
                    .iter()
                    .copied()
                    .eq(view.history[&roots[0]].iter().copied())
            );
            Ok(())
        },
    )
}

fn assert_link_limits() -> TestResult {
    let finite = fixtures::lifecycle(MaxActiveOverride::Millis(500), 1_000, 0, 100)?;
    let infinite = fixtures::lifecycle(MaxActiveOverride::Infinite, 1_000, 0, 100)?;
    let records = fixtures::family_data(finite, infinite, b"waiting", (b"p", b"p"))?
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
            let root = ScopedRecordId {
                partition_key: b"p",
                id: b"child",
            };
            // Two initial, two notification, one member, two cancellation, two final sizing links.
            let plan = assert_scalar_budget(
                view,
                &[root],
                TimeoutLimits::new(100_000, 4, 9, 100, 128 << 20)?,
                TimeoutLimits::new(100_000, 4, 8, 100, 128 << 20)?,
                ActiveTimeoutRejection::Links {
                    count: 9,
                    maximum: 8,
                },
            )?;
            assert_eq!((plan.writes().len(), plan.history().len()), (3, 4));
            assert_eq!(
                plan.history().iter().map(|h| h.kind).collect::<Vec<_>>(),
                [
                    EventKind::Failed,
                    EventKind::ChildFailed,
                    EventKind::Cancelled,
                    EventKind::ChildrenCancelled
                ]
            );
            assert!(plan.writes().iter().all(|w| w.expected_version == 1));
            Ok(())
        },
    )?;
    assert!(records == originals);
    Ok(())
}

fn assert_release_extrema(view: ActiveTimeoutView<'_>) -> TestResult {
    for outbox in [
        ReleaseOutboxState {
            head: MAX_EXACT_MILLIS + 1,
            tail: MAX_EXACT_MILLIS,
        },
        ReleaseOutboxState { head: 0, tail: 0 },
    ] {
        let bounded = ActiveTimeoutView {
            outbox: &outbox,
            ..view
        };
        assert_no_plan(
            expire_at(
                bounded,
                &[ScopedRecordId {
                    partition_key: b"p",
                    id: b"child",
                }],
                TimestampMillis::new(1),
                EntryOrder {
                    identity: LogIdentity::new(1, 7, AppliedPosition::new(2)),
                    first_ordinal: 0,
                },
                (AdmissionLimits::default(), TimeoutLimits::default()),
            ),
            Err(ApplyError::InvalidAuthority),
        )?;
    }
    Ok(())
}
