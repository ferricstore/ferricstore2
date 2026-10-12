//! Claim ownership controls; private unit fixtures certify only pure plans.

use super::{CommittedCommand, EventKind, Fence, LatestEvent, Operation, Outcome};
use super::{TestResult, TimestampMillis, WorkflowRecord, queued, reduce};
use crate::claim::Claim;
use crate::history::HistoryEntry;
use crate::lease::{Lease, LeaseToken};
use wf_types::identity::RecordId;

#[test]
fn claim_and_reclaim_preserve_retained_owners_and_exact_history() -> TestResult {
    for (payload, result, error) in [
        (None, None, None),
        (Some(vec![]), Some(vec![]), Some(vec![])),
        (
            Some(vec![0, 255, 7]),
            Some(vec![255, 0, 8]),
            Some(vec![9, 0, 255]),
        ),
        (Some(vec![0, 255, 7]), None, Some(vec![])),
    ] {
        for (saved, expected_saved) in [
            (None, b"queued".as_slice()),
            (Some(vec![]), b"queued".as_slice()),
            (Some(vec![255, 0, 1]), [255, 0, 1].as_slice()),
        ] {
            for reclaim in [false, true] {
                let record = claim_ownership_record(
                    payload.as_deref(),
                    result.as_deref(),
                    error.as_deref(),
                    saved.as_deref(),
                    reclaim,
                )?;
                let before = record.clone();
                let operation = Claim {
                    expected_state: record.state.clone(),
                    worker: vec![0, 255, 3],
                    token: LeaseToken::new(vec![255, 0, 4])?,
                    lease_ms: 5,
                };
                let command = CommittedCommand {
                    id: record.id.clone(),
                    now: TimestampMillis::new(50),
                    operation: if reclaim {
                        Operation::Reclaim(operation)
                    } else {
                        Operation::Claim(operation)
                    },
                };
                let expected = WorkflowRecord {
                    id: RecordId::new(b"counter-boundary".to_vec())?,
                    workflow_type: b"type".to_vec(),
                    state: b"running".to_vec(),
                    version: 8,
                    fence: Fence::new(12),
                    attempts: 13,
                    created_at: TimestampMillis::new(4),
                    updated_at: TimestampMillis::new(50),
                    next_run_at: Some(TimestampMillis::new(55)),
                    run_state: Some(if reclaim {
                        expected_saved.to_vec()
                    } else {
                        vec![255, 0, 2]
                    }),
                    lease: Some(Lease {
                        worker: vec![0, 255, 3],
                        token: LeaseToken::new(vec![255, 0, 4])?,
                        deadline: TimestampMillis::new(55),
                    }),
                    payload: payload.clone(),
                    result: result.clone(),
                    error: error.clone(),
                    latest_event: LatestEvent {
                        kind: EventKind::Claimed,
                        version: 8,
                        fence: Fence::new(12),
                    },
                };
                let expected_history = HistoryEntry {
                    event_id: "50-8".into(),
                    kind: EventKind::Claimed,
                    occurred_at: TimestampMillis::new(50),
                    version: 8,
                    fence: Fence::new(12),
                    attempts: 13,
                    state: b"running".to_vec(),
                    next_run_at: Some(TimestampMillis::new(55)),
                    lease_deadline: TimestampMillis::new(55),
                    worker: Some(vec![0, 255, 3]),
                    retry: None,
                };
                let plan = reduce(Some(&record), &command)?;
                assert_eq!(plan.outcome(), Outcome::Changed);
                let write = plan.write().ok_or("ownership write absent")?;
                assert_eq!(write.expected_version, Some(7));
                assert_eq!(write.record, expected);
                assert_eq!(plan.history(), Some(&expected_history));
                assert_eq!(plan, reduce(Some(&record), &command)?);
                assert_eq!(record, before);
            }
        }
    }
    Ok(())
}

fn claim_ownership_record(
    payload: Option<&[u8]>,
    result: Option<&[u8]>,
    error: Option<&[u8]>,
    saved: Option<&[u8]>,
    reclaim: bool,
) -> Result<WorkflowRecord, Box<dyn std::error::Error>> {
    let mut record = queued()?;
    record.state = if reclaim {
        b"running".to_vec()
    } else {
        vec![255, 0, 2]
    };
    record.version = 7;
    record.fence = Fence::new(11);
    record.attempts = 13;
    record.created_at = TimestampMillis::new(4);
    record.updated_at = TimestampMillis::new(9);
    record.next_run_at = Some(TimestampMillis::new(50));
    record.run_state = saved.map(<[u8]>::to_vec);
    record.payload = payload.map(<[u8]>::to_vec);
    record.result = result.map(<[u8]>::to_vec);
    record.error = error.map(<[u8]>::to_vec);
    record.latest_event = LatestEvent {
        kind: EventKind::Created,
        version: 7,
        fence: Fence::new(11),
    };
    record.lease = if reclaim {
        Some(Lease {
            worker: vec![255; 1_024],
            token: LeaseToken::new(vec![0; 128])?,
            deadline: TimestampMillis::new(50),
        })
    } else {
        None
    };
    Ok(record)
}
