use super::*;

#[test]
fn touched_indexed_requires_reached_observations() -> Result<(), Box<dyn Error>> {
    let old = record(b"a")?;
    let other = record(b"b")?;
    let command = transition(b"a")?;
    let row = entry(&old, 77);
    with_views(
        &[row, entry(&other, 78)],
        &[&command],
        false,
        |_, touched| assert_parallel_facts(touched, row, &command),
    )?;
    let create = create(b"a")?;
    with_views(&[], &[&create], false, |_, touched| {
        let empty = BTreeMap::new();
        assert_invalid(
            TouchedIndexedGroupView {
                current: &empty,
                ..touched
            },
            &create,
        );
        assert!(matches!(
            decide(touched, &[&create], order(2, 0))?,
            IndexedDecision::Accepted(_)
        ));
        Ok(())
    })?;
    assert_retry_override()
}

fn assert_parallel_facts(
    touched: TouchedIndexedGroupView<'_, '_>,
    row: ClaimRecord<'_>,
    command: &CommittedCommand,
) -> Result<(), Box<dyn Error>> {
    let current = BTreeMap::new();
    let parallel = BTreeMap::new();
    let heads = BTreeMap::new();
    let policies = BTreeMap::new();
    let placements = BTreeMap::new();
    for view in [
        TouchedIndexedGroupView {
            current: &current,
            ..touched
        },
        TouchedIndexedGroupView {
            parallel: &parallel,
            ..touched
        },
        TouchedIndexedGroupView {
            fifo_heads: &heads,
            ..touched
        },
        TouchedIndexedGroupView {
            policies: &policies,
            ..touched
        },
        TouchedIndexedGroupView {
            placements: &placements,
            ..touched
        },
    ] {
        assert_invalid(view, command);
    }
    let due = DueOrder::for_record(&row).ok_or("due absent")?;
    let parallel = BTreeMap::from([(due, false)]);
    assert_invalid(
        TouchedIndexedGroupView {
            parallel: &parallel,
            ..touched
        },
        command,
    );
    let lane = LaneKey::for_record(&row).ok_or("lane absent")?;
    let other = touched
        .current
        .get(&ScopedRecordId {
            partition_key: b"p",
            id: b"b",
        })
        .copied()
        .flatten()
        .ok_or("other canonical point absent")?;
    let heads = BTreeMap::from([(lane, Some((LaneOrder::for_record(&other), other)))]);
    assert_invalid(
        TouchedIndexedGroupView {
            fifo_heads: &heads,
            ..touched
        },
        command,
    );
    assert_current_policy(touched, row, command)?;
    let unused_members = BTreeMap::new();
    let unused_retries = BTreeMap::new();
    assert!(matches!(
        decide(
            TouchedIndexedGroupView {
                fifo_members: &unused_members,
                retry_policies: &unused_retries,
                ..touched
            },
            &[command],
            order(2, 0)
        )?,
        IndexedDecision::Accepted(_)
    ));
    Ok(())
}

#[test]
fn touched_indexed_preserves_member_and_head_authority() -> Result<(), Box<dyn Error>> {
    let a = record(b"a")?;
    let b = record(b"b")?;
    let rows = [entry(&a, 100), entry(&b, 101)];
    let command = claim(b"b")?;
    with_views(&rows, &[&command], true, |full, touched| {
        assert_eq!(
            decide(touched, &[&command], order(2, 0))?,
            complete(full, &[&command], order(2, 0))?
        );
        assert_eq!(
            decide(touched, &[&command], order(2, 0))?,
            IndexedDecision::Rejected(IndexRejection::NotFifoHead)
        );
        let heads = BTreeMap::new();
        assert_invalid(
            TouchedIndexedGroupView {
                fifo_heads: &heads,
                ..touched
            },
            &command,
        );
        let members = BTreeMap::new();
        assert_invalid(
            TouchedIndexedGroupView {
                fifo_members: &members,
                ..touched
            },
            &command,
        );
        let parallel = BTreeMap::new();
        assert_invalid(
            TouchedIndexedGroupView {
                parallel: &parallel,
                ..touched
            },
            &command,
        );
        let head = claim(b"a")?;
        assert_eq!(
            decide(touched, &[&head], order(2, 0))?,
            complete(full, &[&head], order(2, 0))?
        );
        let lane = LaneKey::for_record(&rows[0]).ok_or("lane absent")?;
        let wrong = BTreeMap::from([(lane, Some((LaneOrder::for_record(&rows[1]), rows[0])))]);
        assert_invalid(
            TouchedIndexedGroupView {
                fifo_heads: &wrong,
                ..touched
            },
            &command,
        );
        let update = transition(b"b")?;
        assert!(matches!(
            decide(
                TouchedIndexedGroupView {
                    fifo_heads: &heads,
                    ..touched
                },
                &[&update],
                order(2, 0)
            )?,
            IndexedDecision::Accepted(_)
        ));
        Ok(())
    })?;
    assert_unscheduled_follower()
}

fn assert_unscheduled_follower() -> Result<(), Box<dyn Error>> {
    let head = unscheduled(b"a")?;
    let follower = unscheduled(b"b")?;
    let rows = [entry(&head, 100), entry(&follower, 101)];
    let mut cancel = create(b"b")?;
    cancel.operation = Operation::Cancel(Cancel {
        fence: Fence::new(0),
        lease_token: None,
        expected_state: Some(b"waiting".to_vec()),
        expected_version: Some(1),
        error: None,
    });
    with_views(&rows, &[&cancel], true, |full, touched| {
        let heads = BTreeMap::new();
        let decision = decide(
            TouchedIndexedGroupView {
                fifo_heads: &heads,
                ..touched
            },
            &[&cancel],
            order(2, 0),
        )?;
        assert_eq!(decision, complete(full, &[&cancel], order(2, 0))?);
        let IndexedDecision::Accepted(plan) = decision else {
            return Err("follower cancel rejected".into());
        };
        assert!(matches!(plan.deltas().next().ok_or("delta absent")?.before,
            Some(IndexRows::Fifo { member, head_candidate: None, .. }) if member.id() == b"b"));
        let key = (
            LaneKey::for_record(&rows[1]).ok_or("lane absent")?,
            LaneOrder::for_record(&rows[1]),
        );
        let members = BTreeMap::from([(key, None)]);
        assert_invalid(
            TouchedIndexedGroupView {
                fifo_members: &members,
                ..touched
            },
            &cancel,
        );
        let members = BTreeMap::from([(key, Some(rows[0]))]);
        assert_invalid(
            TouchedIndexedGroupView {
                fifo_members: &members,
                ..touched
            },
            &cancel,
        );
        Ok(())
    })
}

fn assert_retry_override() -> Result<(), Box<dyn Error>> {
    let initial = record(b"a")?;
    let claimed = reduce(Some(&initial), &claim(b"a")?)?
        .into_parts()
        .1
        .ok_or("claim absent")?
        .record;
    let row = entry(&claimed, 100);
    let mut command = create(b"a")?;
    command.operation = Operation::RetryWithPolicy(PolicyRetry {
        guard: LeaseGuard {
            token: LeaseToken::new(b"token".to_vec())?,
            fence: claimed.fence(),
        },
        run_at: None,
        policy_override: RetryOverride::default(),
        jitter: JitterSample::new(0),
        payload: None,
        error: None,
    });
    let installed = InstalledRetryPolicy::new(
        b"job".to_vec(),
        RetryOverride {
            max_retries: Some(0),
            exhausted_to: Some(b"rescue".to_vec()),
            ..RetryOverride::default()
        },
        BTreeMap::new(),
    )?;
    let foreign = InstalledRetryPolicy::new(
        b"foreign".to_vec(),
        RetryOverride::default(),
        BTreeMap::new(),
    )?;
    with_views(&[row], &[&command], false, |_, touched| {
        let partition = PartitionId::new(b"p".to_vec())?;
        let mut item = IndexedBatchItem::new(BatchItem::new(&partition, &command, b"key"));
        item.item.retry_policy = Some(&foreign);
        assert_default_retry(touched, item)?;
        let observed = BTreeMap::from([(b"job".as_slice(), Some(&installed))]);
        let IndexedDecision::Accepted(rescued) = decide(
            TouchedIndexedGroupView {
                retry_policies: &observed,
                ..touched
            },
            &[&command],
            order(20, 4),
        )?
        else {
            return Err("rescue rejected".into());
        };
        let record = &rescued.batch().items()[0]
            .plan()
            .write()
            .ok_or("write absent")?
            .record;
        assert_eq!(record.state(), b"rescue");
        assert_eq!(record.next_run_at(), Some(TimestampMillis::new(10)));
        assert_eq!(record.attempts(), 1);
        assert_eq!(
            rescued
                .metadata()
                .next()
                .ok_or("metadata absent")?
                .state_enter_seq,
            85_899_345_924
        );
        assert_eq!(
            rescued.batch().items()[0]
                .plan()
                .history()
                .ok_or("history absent")?
                .kind,
            EventKind::Retry
        );
        Ok(())
    })
}

fn assert_default_retry(
    touched: TouchedIndexedGroupView<'_, '_>,
    item: IndexedBatchItem<'_>,
) -> Result<(), Box<dyn Error>> {
    let foreign = item.item.retry_policy.ok_or("foreign item policy absent")?;
    let IndexedDecision::Accepted(default) = decide_touched_indexed_batch(
        touched,
        &[item],
        order(20, 4),
        b"entry",
        AdmissionLimits::default(),
    )?
    else {
        return Err("defaults rejected".into());
    };
    let record = &default.batch().items()[0]
        .plan()
        .write()
        .ok_or("write absent")?
        .record;
    assert_eq!(record.state(), b"queued");
    assert_eq!(record.next_run_at(), Some(TimestampMillis::new(810)));
    assert_eq!(record.attempts(), 1);
    assert_eq!(
        default
            .metadata()
            .next()
            .ok_or("metadata absent")?
            .state_enter_seq,
        100
    );
    let unknown = BTreeMap::new();
    assert_invalid(
        TouchedIndexedGroupView {
            retry_policies: &unknown,
            ..touched
        },
        item.item.command,
    );
    let wrong = BTreeMap::from([(b"job".as_slice(), Some(foreign))]);
    assert_invalid(
        TouchedIndexedGroupView {
            retry_policies: &wrong,
            ..touched
        },
        item.item.command,
    );
    Ok(())
}

fn assert_current_policy(
    touched: TouchedIndexedGroupView<'_, '_>,
    row: ClaimRecord<'_>,
    command: &CommittedCommand,
) -> Result<(), Box<dyn Error>> {
    let broken = ClaimRecord {
        partition_key: b"wrong",
        ..row
    };
    let current = BTreeMap::from([(ScopedRecordId::for_record(&row), Some(broken))]);
    assert_invalid(
        TouchedIndexedGroupView {
            current: &current,
            ..touched
        },
        command,
    );
    let foreign = prepare_install(
        None,
        0,
        &InstallCommand::low_level(b"foreign".to_vec(), PolicyPatch::default()),
        PolicyLimits::default(),
    )?;
    let policies = BTreeMap::from([(b"job".as_slice(), Some(foreign.policy()))]);
    assert_invalid(
        TouchedIndexedGroupView {
            policies: &policies,
            ..touched
        },
        command,
    );
    Ok(())
}
