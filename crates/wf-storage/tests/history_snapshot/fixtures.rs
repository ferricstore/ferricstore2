use super::*;
use wf_domain::claim::Claim;
use wf_domain::workflow::Create;
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};

pub(super) const GOLDEN: &str = concat!(
    "465748530001000000b54657535200020000000167000000017a0000000174000000017001000000",
    "00000000000000000900000003000000016900000005656d61696c0000000772756e6e696e670000",
    "00000000000300000000000000020000000000000000000000000000000000000000000000020100",
    "00000000007532010000000663686172676501000000037700ff000000077700ff3a323a32000000",
    "0000007532010000000200ff00000d000000000000000300000000000000020000006c4657534800",
    "020000000167000000017a00000001740000000170000000016900000003322d330d000000000000",
    "00020000000000000003000000000000000200000000000000000000000772756e6e696e67010000",
    "000000007532000000000000753201000000037700ff00"
);

pub(super) fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    ))
}

pub(super) fn caps(
    encoded: usize,
    owned: usize,
    workspace: usize,
) -> TestResult<HistorySnapshotLimits> {
    Ok(HistorySnapshotLimits::new(
        AdmissionLimits::new(32, 8192, 4)?,
        encoded,
        owned,
        workspace,
    )?)
}
pub(super) fn limits() -> TestResult<HistorySnapshotLimits> {
    caps(8192, 16384, WORKSPACE)
}

fn claimed() -> TestResult<WorkflowRecord> {
    let create = CommittedCommand {
        id: RecordId::new(b"i".to_vec())?,
        now: TimestampMillis::new(0),
        operation: Operation::Create(Create {
            workflow_type: b"email".to_vec(),
            state: b"authorize".to_vec(),
            run_at: None,
            payload: Some(b"old".to_vec()),
        }),
    };
    let (_, Some(write), _) = reduce(None, &create)?.into_parts() else {
        return Err("fixture Create".into());
    };
    let record = write.record;
    let claim = CommittedCommand {
        id: record.id().clone(),
        now: TimestampMillis::new(1),
        operation: Operation::Claim(Claim {
            expected_state: b"authorize".to_vec(),
            worker: b"w\0\xff".to_vec(),
            token: LeaseToken::new(b"lease".to_vec())?,
            lease_ms: 30_000,
        }),
    };
    let (_, Some(write), _) = reduce(Some(&record), &claim)?.into_parts() else {
        return Err("fixture Claim".into());
    };
    Ok(write.record)
}

pub(super) fn step(payload: Option<&[u8]>) -> TestResult<StoredWorkflowHistorySnapshot> {
    let record = claimed()?;
    let command = CommittedCommand {
        id: record.id().clone(),
        now: TimestampMillis::new(2),
        operation: Operation::StepContinue(StepContinue {
            guard: LeaseGuard {
                token: LeaseToken::new(b"lease".to_vec())?,
                fence: record.fence(),
            },
            from: b"authorize".to_vec(),
            to: b"charge".to_vec(),
            worker: None,
            lease_ms: 30_000,
            payload: payload.map(<[u8]>::to_vec),
        }),
    };
    let (_, Some(write), Some(event)) = reduce(Some(&record), &command)?.into_parts() else {
        return Err("fixture Step".into());
    };
    Ok(StoredWorkflowHistorySnapshot {
        event,
        postimage: StoredWorkflowRecord {
            tenant: scope()?.tenant().clone(),
            partition: PartitionId::new(b"p".to_vec())?,
            record: write.record,
            ordering: OrderingMetadata {
                priority: 1,
                state_enter_seq: (9_u128 << 32) | 3,
            },
        },
    })
}

pub(super) fn encoded(snapshot: &StoredWorkflowHistorySnapshot) -> TestResult<Vec<u8>> {
    let result =
        encode_history_snapshot(&scope()?, &snapshot.postimage, &snapshot.event, limits()?);
    assert!(
        result.is_ok(),
        "healthy history snapshot: {:?}",
        result.as_ref().err()
    );
    Ok(result?)
}

pub(super) fn decoded(
    bytes: &[u8],
    limits: HistorySnapshotLimits,
) -> TestResult<StoredWorkflowHistorySnapshot> {
    Ok(decode_history_snapshot(
        bytes,
        &scope()?,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(b"i".to_vec())?,
        limits,
    )?)
}

pub(super) fn record_bytes(bytes: &[u8]) -> TestResult<&[u8]> {
    let length = usize::try_from(u32::from_be_bytes(
        bytes.get(6..10).ok_or("record length")?.try_into()?,
    ))?;
    Ok(bytes.get(10..10 + length).ok_or("record frame")?)
}

/// Restoration controls exercise enum representation, not claimed command behavior.
pub(super) fn kind(
    snapshot: &StoredWorkflowHistorySnapshot,
    kind: EventKind,
) -> TestResult<StoredWorkflowHistorySnapshot> {
    let record = &snapshot.postimage.record;
    let record = restore_record(
        WorkflowRecordData {
            id: record.id().clone(),
            workflow_type: record.workflow_type().to_vec(),
            state: record.state().to_vec(),
            version: record.version(),
            fence: record.fence(),
            attempts: record.attempts(),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
            next_run_at: record.next_run_at(),
            run_state: record.run_state().map(<[u8]>::to_vec),
            lease: record.lease().cloned(),
            payload: record.payload().map(<[u8]>::to_vec),
            result: record.result().map(<[u8]>::to_vec),
            error: record.error().map(<[u8]>::to_vec),
            latest_kind: kind,
            latest_version: record.version(),
            latest_fence: record.fence(),
        },
        AdmissionLimits::default(),
    )?;
    let mut event = snapshot.event.clone();
    event.kind = kind;
    event.retry = None;
    Ok(StoredWorkflowHistorySnapshot {
        event,
        postimage: StoredWorkflowRecord {
            record,
            ..snapshot.postimage.clone()
        },
    })
}

pub(super) fn hex(value: &str) -> TestResult<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return Err("odd hex".into());
    }
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}
