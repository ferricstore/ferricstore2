//! Independent deterministic FIFO-head/priority/due oracle over many snapshots.
#![forbid(unsafe_code)]

use std::cmp::Reverse;
use std::collections::BTreeMap;

use wf_domain::claim::{
    AuthoritativeClaimView, Claim, ClaimRecord, DueOrder, FifoIndex, LaneKey, LaneOrder,
    ScopedRecordId, SelectionAction, SelectionRequest, select,
};
use wf_domain::lease::LeaseToken;
use wf_domain::workflow::{
    CommittedCommand, Create, Operation, WorkflowRecord, WorkflowStatus, reduce,
};
use wf_types::identity::RecordId;
use wf_types::version::TimestampMillis;

const PARTITIONS: [&[u8]; 4] = [b"p0", b"p1", b"p2", b"p3"];

type TestError = Box<dyn std::error::Error>;
type TestResult = Result<(), TestError>;

fn record(seed: u8, index: u8) -> Result<WorkflowRecord, TestError> {
    let due = (u64::from(seed) * 11 + u64::from(index) * 17) % 160;
    let create = CommittedCommand {
        id: RecordId::new(vec![seed, index])?,
        now: TimestampMillis::new(0),
        operation: Operation::Create(Create {
            workflow_type: b"model".to_vec(),
            state: if index < 12 {
                b"queued".to_vec()
            } else {
                b"parallel".to_vec()
            },
            run_at: Some(TimestampMillis::new(due)),
            payload: None,
        }),
    };
    let record = reduce(None, &create)?
        .into_parts()
        .1
        .ok_or("required test fixture value absent")?
        .record;
    Ok(if index < 12 && index.is_multiple_of(5) {
        reduce(
            Some(&record),
            &CommittedCommand {
                id: record.id().clone(),
                now: TimestampMillis::new(due),
                operation: Operation::Claim(Claim {
                    expected_state: b"queued".to_vec(),
                    worker: b"worker".to_vec(),
                    token: LeaseToken::new(vec![seed, index])?,
                    lease_ms: 20,
                }),
            },
        )?
        .into_parts()
        .1
        .ok_or("required test fixture value absent")?
        .record
    } else {
        record
    })
}

fn expected(entries: &[ClaimRecord<'_>], reclaim: bool) -> Result<Vec<(Vec<u8>, bool)>, TestError> {
    assert!(
        entries
            .iter()
            .all(|entry| entry.record.next_run_at().is_some())
    );
    let mut lanes: BTreeMap<&[u8], Vec<usize>> = BTreeMap::new();
    for (index, entry) in entries[..12].iter().enumerate() {
        lanes.entry(entry.partition_key).or_default().push(index);
    }
    let mut candidates: Vec<_> = (12..entries.len()).collect();
    for lane in lanes.values() {
        candidates.push(
            *lane
                .iter()
                .min_by_key(|index| {
                    let entry = entries[**index];
                    (
                        if entry.record.status() == WorkflowStatus::Running {
                            -1
                        } else {
                            0
                        },
                        entry.state_enter_seq,
                        entry.record.id().as_bytes(),
                    )
                })
                .ok_or("required test fixture value absent")?,
        );
    }
    candidates.retain(|index| {
        let record = entries[*index].record;
        record.next_run_at().map_or(0, TimestampMillis::get) <= 100
            && (reclaim || record.status() != WorkflowStatus::Running)
    });
    candidates.sort_by_key(|index| {
        let entry = entries[*index];
        (
            Reverse(entry.priority),
            entry.record.next_run_at().map_or(0, TimestampMillis::get),
            entry.record.id().as_bytes(),
            entry.partition_key,
        )
    });
    Ok(candidates
        .into_iter()
        .take(7)
        .map(|index| {
            let entry = entries[index];
            (
                entry.record.id().as_bytes().to_vec(),
                entry.record.status() == WorkflowStatus::Running,
            )
        })
        .collect())
}

#[test]
fn generated_snapshots_match_independent_head_and_due_model() -> TestResult {
    for seed in 0..64_u8 {
        let records: Vec<_> = (0..24)
            .map(|index| record(seed, index))
            .collect::<Result<_, _>>()?;
        let entries: Vec<_> = records
            .iter()
            .enumerate()
            .map(|(index, record)| {
                Ok(ClaimRecord {
                    record,
                    partition_key: PARTITIONS[index % 4],
                    priority: if index < 12 {
                        0
                    } else {
                        [0_u8, 1, 2][index % 3]
                    },
                    state_enter_seq: (u128::from(seed) << 64)
                        + u128::from(u64::try_from((index * 7) % 25)?),
                })
            })
            .collect::<Result<_, TestError>>()?;
        let mut fifo = FifoIndex::new();
        for entry in &entries[..12] {
            fifo.entry(LaneKey::for_record(entry).ok_or("required test fixture value absent")?)
                .or_default()
                .insert(LaneOrder::for_record(entry), *entry);
        }
        let parallel = entries[12..]
            .iter()
            .map(|entry| {
                Ok((
                    DueOrder::for_record(entry).ok_or("required test fixture value absent")?,
                    *entry,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, TestError>>()?;
        let canonical = BTreeMap::from_iter(
            entries
                .iter()
                .map(|entry| (ScopedRecordId::for_record(entry), *entry)),
        );
        let view = AuthoritativeClaimView {
            group: b"g",
            revision: u64::from(seed),
            records: &canonical,
            parallel: &parallel,
            fifo: &fifo,
        };
        let mut request = SelectionRequest::new(b"g", b"model", TimestampMillis::new(100));
        request.limit = 7;
        request.reclaim_expired = seed % 2 == 0;
        let selected = select(&view, request, None)?;
        let actual: Vec<_> = selected
            .records()
            .iter()
            .map(|selected| {
                (
                    selected.record.id().as_bytes().to_vec(),
                    selected.action == SelectionAction::Reclaim,
                )
            })
            .collect();
        assert_eq!(
            actual,
            expected(&entries, request.reclaim_expired)?,
            "seed {seed}"
        );
        let repeated = select(&view, request, None)?;
        assert_eq!(selected.records(), repeated.records());
        assert_eq!(selected.scanned(), repeated.scanned());
        assert_eq!(selected.stop(), repeated.stop());
        assert_eq!(
            selected.continuation().is_some(),
            repeated.continuation().is_some()
        );
        if let (Some(first), Some(second)) = (selected.continuation(), repeated.continuation()) {
            assert_eq!(
                select(&view, request, Some(first))?.records(),
                select(&view, request, Some(second))?.records()
            );
        }
        assert_eq!(selected.head_reads(), 4);
    }
    Ok(())
}
