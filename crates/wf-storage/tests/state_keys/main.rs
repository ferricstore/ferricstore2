//! Engine-independent physical bytes; no backend or durability is exercised.

#![forbid(unsafe_code)]

use wf_domain::claim::{Claim, ClaimRecord, LaneKey};
use wf_domain::history::{EventKind, HistoryEntry};
use wf_domain::lease::LeaseToken;
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::{CommittedCommand, Create, Operation, WorkflowRecord, reduce};
use wf_types::identity::{GroupId, RecordId, RecoveryId, TenantId};
use wf_types::version::{Fence, TimestampMillis};

mod ordered;
mod point;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn scope(group: &[u8], recovery: &[u8], tenant: &[u8]) -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(tenant.to_vec())?,
        GroupId::new(group.to_vec())?,
        RecoveryId::new(recovery.to_vec())?,
    ))
}

fn standard_scope() -> TestResult<RequestScope> {
    scope(b"g", b"z", b"t")
}

fn hex(value: &str) -> TestResult<Vec<u8>> {
    let (pairs, tail) = value.as_bytes().as_chunks::<2>();
    if !tail.is_empty() {
        return Err("odd hex fixture".into());
    }
    pairs
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn record(id: &[u8], running: bool) -> TestResult<WorkflowRecord> {
    let command = CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(1),
        operation: Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: Some(TimestampMillis::new(1)),
            payload: None,
        }),
    };
    let created = reduce(None, &command)?
        .into_parts()
        .1
        .ok_or("created write absent")?
        .record;
    if !running {
        return Ok(created);
    }
    Ok(reduce(
        Some(&created),
        &CommittedCommand {
            id: command.id,
            now: TimestampMillis::new(2),
            operation: Operation::Claim(Claim {
                expected_state: b"q".to_vec(),
                worker: b"w".to_vec(),
                token: LeaseToken::new(b"tok".to_vec())?,
                lease_ms: 10,
            }),
        },
    )?
    .into_parts()
    .1
    .ok_or("claimed write absent")?
    .record)
}

fn entry(record: &WorkflowRecord, sequence: u128) -> ClaimRecord<'_> {
    ClaimRecord {
        record,
        partition_key: b"p",
        priority: 2,
        state_enter_seq: sequence,
    }
}

fn lane() -> LaneKey<'static> {
    LaneKey {
        workflow_type: b"r",
        state: b"q",
        partition_key: b"p",
    }
}

// Mathematical history data, not a claim that every combination was generated.
fn history(event_id: &str, version: u64) -> HistoryEntry {
    HistoryEntry {
        event_id: event_id.to_owned(),
        kind: EventKind::Created,
        occurred_at: TimestampMillis::new(0),
        version,
        fence: Fence::new(0),
        attempts: 0,
        state: b"q".to_vec(),
        next_run_at: None,
        lease_deadline: TimestampMillis::new(0),
        worker: None,
        retry: None,
    }
}
