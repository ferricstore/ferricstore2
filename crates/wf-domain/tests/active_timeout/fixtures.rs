//! Shared exact owned fixture construction; no source or storage adapter.
use super::*;

pub(super) fn lifecycle(
    active: MaxActiveOverride,
    ttl_ms: u64,
    hot: u32,
    total: u32,
) -> TestResult<CapturedLifecycle> {
    Ok(capture_lifecycle(
        None,
        b"job",
        b"queued",
        LifecycleOverrides {
            active,
            retention: RetentionOverride::default(),
            history_hot_max_events: None,
        },
        LifecycleDefaults {
            ttl_ms,
            history_hot_max_events: hot,
            history_max_events: total,
            max_history_hot_max_events: 10_000,
            max_history_max_events: 1_000_000,
        },
    )?)
}

pub(super) fn family_data(
    captured: CapturedLifecycle,
    parent_captured: CapturedLifecycle,
    parent_state: &[u8],
    partitions: (&[u8], &[u8]),
) -> TestResult<Vec<ActiveRecordData>> {
    let parent_next = (parent_state == b"running").then_some(TimestampMillis::new(50));
    let mut parent = fixture(
        b"parent",
        partitions.0,
        parent_state,
        TimestampMillis::new(0),
        parent_next,
        parent_captured,
    )?;
    let explicit = (partitions.0 != partitions.1)
        .then(|| PartitionId::new(partitions.1.to_vec()))
        .transpose()?;
    parent.child_groups.insert(
        b"g".to_vec(),
        ChildGroup {
            wait: ChildWait::All,
            on_child_failed: ChildFailure::FailParent,
            on_parent_closed: ParentClosed::Cancel,
            success_state: b"children_done".to_vec(),
            failure_state: b"children_failed".to_vec(),
            request_hash: b"hash".to_vec(),
            children: [b"child".as_slice(), b"sibling"]
                .into_iter()
                .map(|id| {
                    Ok((
                        RecordId::new(id.to_vec())?,
                        ChildMember {
                            partition: explicit.clone(),
                            status: ChildStatus::Running,
                            result: None,
                        },
                    ))
                })
                .collect::<TestResult<_>>()?,
            summary: ChildSummary {
                total: 2,
                completed: 0,
                failed: 0,
                cancelled: 0,
            },
            resolved: None,
        },
    );
    let mut records = vec![parent];
    for id in [b"child".as_slice(), b"sibling"] {
        let mut child = fixture(
            id,
            partitions.1,
            b"running",
            TimestampMillis::new(0),
            Some(TimestampMillis::new(50)),
            captured,
        )?;
        child.parent = Some(ParentLink {
            id: b"parent".to_vec(),
            partition: (partitions.0 != partitions.1)
                .then(|| PartitionId::new(partitions.0.to_vec()))
                .transpose()?,
        });
        records.push(child);
    }
    Ok(records)
}

pub(super) fn overlay_records(captured: CapturedLifecycle) -> TestResult<Vec<ActiveRecord>> {
    let mut parent = fixture(
        b"parent",
        b"p",
        b"queued",
        TimestampMillis::new(0),
        Some(TimestampMillis::new(0)),
        captured,
    )?;
    parent.child_groups.insert(
        b"g".to_vec(),
        ChildGroup {
            wait: ChildWait::None,
            on_child_failed: ChildFailure::Ignore,
            on_parent_closed: ParentClosed::Cancel,
            success_state: b"done".to_vec(),
            failure_state: b"bad".to_vec(),
            request_hash: Vec::new(),
            children: [(
                RecordId::new(b"child".to_vec())?,
                ChildMember {
                    partition: None,
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
            resolved: Some(ChildResolution::Success),
        },
    );
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
    Ok(vec![
        restore_active(parent, AdmissionLimits::default())?,
        restore_active(child, AdmissionLimits::default())?,
    ])
}
