//! Source-shaped atomic group batches and immutable admission boundaries.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::claim::{
    AuthoritativeClaimView, Claim, ClaimRecord, DueOrder, ScopedRecordId, SelectionRequest, select,
};
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::policy::{InstalledRetryPolicy, RetryOverride};
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::{
    ApplyError, BatchDecision, BatchItem, BatchRejection, GroupView, decide_group_batch,
};
use wf_domain::workflow::{
    CommittedCommand, Create, DomainError, Operation, WorkflowRecord, reduce,
};
use wf_types::identity::{GroupId, PartitionId, RecordId};
use wf_types::version::{Fence, TimestampMillis};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn command(
    id: &[u8],
    now: u64,
    operation: Operation,
) -> Result<CommittedCommand, Box<dyn std::error::Error>> {
    Ok(CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(now),
        operation,
    })
}

fn claimed(id: &[u8]) -> Result<WorkflowRecord, Box<dyn std::error::Error>> {
    let create = command(
        id,
        1_000,
        Operation::Create(Create {
            workflow_type: b"bulk-retry-many".to_vec(),
            state: b"queued".to_vec(),
            run_at: Some(TimestampMillis::new(1_000)),
            payload: None,
        }),
    )?;
    let record = reduce(None, &create)?
        .into_parts()
        .1
        .ok_or("missing create")?
        .record;
    let claim = command(
        id,
        1_000,
        Operation::Claim(Claim {
            expected_state: b"queued".to_vec(),
            worker: b"worker-retry".to_vec(),
            token: LeaseToken::new(id.to_vec())?,
            lease_ms: 30_000,
        }),
    )?;
    Ok(reduce(Some(&record), &claim)?
        .into_parts()
        .1
        .ok_or("missing claim")?
        .record)
}

fn retry(
    record: &WorkflowRecord,
    fence: Fence,
) -> Result<CommittedCommand, Box<dyn std::error::Error>> {
    command(
        record.id().as_bytes(),
        2_000,
        Operation::Retry(Retry {
            guard: LeaseGuard {
                token: record.lease().ok_or("lease absent")?.token.clone(),
                fence,
            },
            run_at: RetryTime::Explicit(TimestampMillis::new(2_000)),
            payload: None,
            error: Some(b"retry-error".to_vec()),
        }),
    )
}

fn assert_reclaimable(records: &[&WorkflowRecord], partition: &PartitionId) -> TestResult {
    let entries: Vec<_> = records
        .iter()
        .zip([1_u128, 2])
        .map(|(record, sequence)| ClaimRecord {
            record,
            partition_key: partition.as_bytes(),
            priority: 0,
            state_enter_seq: sequence,
        })
        .collect();
    let parallel = BTreeMap::from_iter(
        entries
            .iter()
            .filter_map(|entry| DueOrder::for_record(entry).map(|due| (due, *entry))),
    );
    let current = BTreeMap::from_iter(
        entries
            .iter()
            .map(|entry| (ScopedRecordId::for_record(entry), *entry)),
    );
    let fifo = BTreeMap::new();
    let view = AuthoritativeClaimView {
        group: b"group",
        revision: 1,
        records: &current,
        parallel: &parallel,
        fifo: &fifo,
    };
    let mut request =
        SelectionRequest::new(b"group", b"bulk-retry-many", TimestampMillis::new(2_000));
    request.limit = 2;
    let selected = select(&view, request, None)?;
    let mut ids: Vec<_> = selected
        .records()
        .iter()
        .map(|selected| selected.record.id().as_bytes())
        .collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        [b"retry-many-a".as_slice(), b"retry-many-b".as_slice()]
    );
    for selected in selected.records() {
        let claimed = reduce(
            Some(selected.record),
            &command(
                selected.record.id().as_bytes(),
                2_000,
                Operation::Claim(Claim {
                    expected_state: b"queued".to_vec(),
                    worker: b"worker-retry-b".to_vec(),
                    token: LeaseToken::new(b"next-token".to_vec())?,
                    lease_ms: 30_000,
                }),
            )?,
        )?
        .into_parts()
        .1
        .ok_or("reclaim write absent")?
        .record;
        assert_eq!(claimed.attempts(), 1);
        assert_eq!(
            claimed.lease().ok_or("reclaim lease absent")?.worker,
            b"worker-retry-b"
        );
    }
    Ok(())
}

#[test]
fn retry_many_preserves_order_attempts_and_atomic_late_guard_rejection() -> TestResult {
    // Source a7055349acfdcd64:618-669 and fc37a839bdb1b10d:671-709.
    // Exact physical error_ref serialization remains the M2/private-codec gate.
    let group = GroupId::new(b"group".to_vec())?;
    let partition = PartitionId::new(b"tenant-retry-many".to_vec())?;
    let first = claimed(b"retry-many-a")?;
    let second = claimed(b"retry-many-b")?;
    let original = (first.clone(), second.clone());
    let records = BTreeMap::from([
        (
            ScopedRecordId {
                partition_key: partition.as_bytes(),
                id: first.id().as_bytes(),
            },
            &first,
        ),
        (
            ScopedRecordId {
                partition_key: partition.as_bytes(),
                id: second.id().as_bytes(),
            },
            &second,
        ),
    ]);
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let view = GroupView {
        group: &group,
        placements: &placements,
        records: &records,
    };
    let one = retry(&first, first.fence())?;
    let two = retry(&second, second.fence())?;
    let items = [
        BatchItem::new(&partition, &two, b"key-b"),
        BatchItem::new(&partition, &one, b"key-a"),
    ];
    for items in [items, [items[1], items[0]]] {
        let BatchDecision::Accepted(plan) =
            decide_group_batch(view, &items, b"committed-batch", AdmissionLimits::default())?
        else {
            return Err("valid batch rejected".into());
        };
        assert_eq!(
            plan,
            match decide_group_batch(view, &items, b"committed-batch", AdmissionLimits::default())?
            {
                BatchDecision::Accepted(plan) => plan,
                BatchDecision::Rejected(_) => return Err("replay rejected".into()),
            }
        );
        let next: Vec<_> = plan
            .items()
            .iter()
            .filter_map(|item| item.plan().write())
            .map(|write| &write.record)
            .collect();
        assert_eq!(next.len(), 2);
        assert_eq!(
            next.iter()
                .map(|record| record.id().as_bytes())
                .collect::<Vec<_>>(),
            items.map(|item| item.command.id.as_bytes())
        );
        assert!(
            next.iter()
                .all(|record| record.state() == b"queued" && record.attempts() == 1)
        );
        assert!(
            next.iter()
                .all(|record| record.error() == Some(b"retry-error".as_slice()))
        );
        assert!(
            next.iter()
                .all(|record| record.lease().is_none() && record.version() == 3)
        );
        assert_reclaimable(&next, &partition)?;
        assert_eq!(
            plan.items()
                .iter()
                .filter(|item| item.plan().history().is_some())
                .count(),
            2
        );
    }
    let stale = retry(&second, Fence::new(second.fence().get() + 1))?;
    let invalid = [
        BatchItem::new(&partition, &one, b"key-a"),
        BatchItem::new(&partition, &stale, b"key-b"),
    ];
    assert_eq!(
        decide_group_batch(view, &invalid, b"batch", AdmissionLimits::default())?,
        BatchDecision::Rejected(BatchRejection::Domain(DomainError::StaleLease))
    );
    assert_eq!((first, second), original);
    assert_eq!(original.0.state(), b"running");
    assert_eq!(original.1.state(), b"running");
    assert_eq!((original.0.version(), original.1.version()), (2, 2));
    Ok(())
}

#[test]
fn same_group_allows_two_partitions_but_cross_group_and_duplicate_id_reject() -> TestResult {
    let group = GroupId::new(b"a".to_vec())?;
    let other_group = GroupId::new(b"b".to_vec())?;
    let p = PartitionId::new(b"p".to_vec())?;
    let q = PartitionId::new(b"q".to_vec())?;
    let a = claimed(b"a")?;
    let b = claimed(b"b")?;
    let records = BTreeMap::from([
        (
            ScopedRecordId {
                partition_key: p.as_bytes(),
                id: a.id().as_bytes(),
            },
            &a,
        ),
        (
            ScopedRecordId {
                partition_key: q.as_bytes(),
                id: b.id().as_bytes(),
            },
            &b,
        ),
    ]);
    let mut placements = BTreeMap::from([(p.clone(), group.clone()), (q.clone(), group.clone())]);
    let ca = retry(&a, a.fence())?;
    let cb = retry(&b, b.fence())?;
    let items = [BatchItem::new(&p, &ca, b"a"), BatchItem::new(&q, &cb, b"b")];
    assert!(matches!(
        decide_group_batch(
            GroupView {
                group: &group,
                records: &records,
                placements: &placements
            },
            &items,
            b"batch",
            AdmissionLimits::default()
        )?,
        BatchDecision::Accepted(_)
    ));
    placements.insert(q.clone(), other_group);
    assert_eq!(
        decide_group_batch(
            GroupView {
                group: &group,
                records: &records,
                placements: &placements
            },
            &items,
            b"batch",
            AdmissionLimits::default()
        )?,
        BatchDecision::Rejected(BatchRejection::CrossGroup)
    );
    placements.insert(q.clone(), group.clone());
    let duplicate = [BatchItem::new(&p, &ca, b"a"), BatchItem::new(&q, &ca, b"a")];
    assert_eq!(
        decide_group_batch(
            GroupView {
                group: &group,
                records: &records,
                placements: &placements
            },
            &duplicate,
            b"batch",
            AdmissionLimits::default()
        )?,
        BatchDecision::Rejected(BatchRejection::DuplicateId)
    );
    placements.clear();
    assert_eq!(
        decide_group_batch(
            GroupView {
                group: &group,
                records: &records,
                placements: &placements
            },
            &items,
            b"batch",
            AdmissionLimits::default()
        ),
        Err(ApplyError::InvalidAuthority)
    );
    Ok(())
}

#[test]
fn actual_inline_encoded_and_authoritative_bytes_are_bounded_before_copying() -> TestResult {
    let group = GroupId::new(b"g".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let records = BTreeMap::new();
    let view = GroupView {
        group: &group,
        placements: &placements,
        records: &records,
    };
    let create = command(
        b"id",
        0,
        Operation::Create(Create {
            workflow_type: b"t".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: Some(vec![1; 5]),
        }),
    )?;
    let limits = AdmissionLimits::new(4, 4_096, 2)?;
    let item = [BatchItem::new(&partition, &create, b"key")];
    assert!(matches!(
        decide_group_batch(view, &item, b"entry", limits)?,
        BatchDecision::Rejected(BatchRejection::Admission(_))
    ));
    let no_payload = command(
        b"id",
        0,
        Operation::Create(Create {
            workflow_type: b"t".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: Some(vec![]),
        }),
    )?;
    let valid = [BatchItem::new(&partition, &no_payload, b"key")];
    assert!(matches!(
        decide_group_batch(view, &valid, b"entry", limits)?,
        BatchDecision::Accepted(_)
    ));
    assert!(matches!(
        decide_group_batch(view, &valid, &[0; 4_097], limits)?,
        BatchDecision::Rejected(BatchRejection::Admission(_))
    ));
    let long_key = vec![0; 65_536];
    let invalid_key = [BatchItem::new(&partition, &no_payload, &long_key)];
    assert!(matches!(
        decide_group_batch(view, &invalid_key, b"entry", limits)?,
        BatchDecision::Rejected(BatchRejection::Admission(_))
    ));
    let (_, write, _) = reduce(None, &create)?.into_parts();
    let record = write.ok_or("missing record")?.record;
    let records = BTreeMap::from([(
        ScopedRecordId {
            partition_key: partition.as_bytes(),
            id: record.id().as_bytes(),
        },
        &record,
    )]);
    assert_eq!(
        decide_group_batch(
            GroupView {
                group: &group,
                placements: &placements,
                records: &records
            },
            &valid,
            b"entry",
            limits
        ),
        Err(ApplyError::InvalidAuthority)
    );
    Ok(())
}

#[test]
fn oversized_copied_authoritative_retry_layer_rejects_even_if_command_overrides_it() -> TestResult {
    use wf_domain::retry::{JitterSample, PolicyRetry};
    let group = GroupId::new(b"g".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = claimed(b"a")?;
    let records = BTreeMap::from([(
        ScopedRecordId {
            partition_key: partition.as_bytes(),
            id: record.id().as_bytes(),
        },
        &record,
    )]);
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let policy = InstalledRetryPolicy::new(
        b"bulk-retry-many".to_vec(),
        RetryOverride {
            exhausted_to: Some(vec![b'x'; 128]),
            ..RetryOverride::default()
        },
        BTreeMap::new(),
    )?;
    let command = command(
        b"a",
        2_000,
        Operation::RetryWithPolicy(PolicyRetry {
            guard: LeaseGuard {
                token: record.lease().ok_or("lease")?.token.clone(),
                fence: record.fence(),
            },
            run_at: None,
            jitter: JitterSample::new(0),
            payload: None,
            error: None,
            policy_override: RetryOverride {
                max_retries: Some(0),
                exhausted_to: Some(b"failed".to_vec()),
                ..RetryOverride::default()
            },
        }),
    )?;
    let mut item = BatchItem::new(&partition, &command, b"key");
    item.retry_policy = Some(&policy);
    assert_eq!(
        decide_group_batch(
            GroupView {
                group: &group,
                records: &records,
                placements: &placements
            },
            &[item],
            b"entry",
            AdmissionLimits::new(32, 256, 1)?
        ),
        Err(ApplyError::InvalidAuthority)
    );
    let foreign = InstalledRetryPolicy::new(
        b"foreign-type".to_vec(),
        RetryOverride::default(),
        BTreeMap::new(),
    )?;
    item.retry_policy = Some(&foreign);
    assert_eq!(
        decide_group_batch(
            GroupView {
                group: &group,
                records: &records,
                placements: &placements
            },
            &[item],
            b"entry",
            AdmissionLimits::default()
        ),
        Err(ApplyError::InvalidAuthority)
    );
    assert_eq!(
        wf_domain::workflow::reduce_with_policy(Some(&record), &command, Some(&foreign)),
        Err(DomainError::PolicyTypeMismatch)
    );
    Ok(())
}

#[test]
fn aggregate_touched_authority_is_bounded_before_any_record_clone() -> TestResult {
    let group = GroupId::new(b"g".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let a = claimed(b"a")?;
    let b = claimed(b"b")?;
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let records = BTreeMap::from([
        (
            ScopedRecordId {
                partition_key: partition.as_bytes(),
                id: a.id().as_bytes(),
            },
            &a,
        ),
        (
            ScopedRecordId {
                partition_key: partition.as_bytes(),
                id: b.id().as_bytes(),
            },
            &b,
        ),
    ]);
    let ca = retry(&a, a.fence())?;
    let cb = retry(&b, b.fence())?;
    let items = [
        BatchItem::new(&partition, &ca, b"a"),
        BatchItem::new(&partition, &cb, b"b"),
    ];
    assert!(matches!(
        decide_group_batch(
            GroupView {
                group: &group,
                records: &records,
                placements: &placements
            },
            &items,
            b"tiny",
            AdmissionLimits::new(64, 2_300, 2)?
        )?,
        BatchDecision::Rejected(BatchRejection::Admission(_))
    ));
    assert!(matches!(
        decide_group_batch(
            GroupView {
                group: &group,
                records: &records,
                placements: &placements
            },
            &items[..1],
            b"tiny",
            AdmissionLimits::new(64, 2_300, 2)?
        )?,
        BatchDecision::Accepted(_)
    ));
    Ok(())
}
