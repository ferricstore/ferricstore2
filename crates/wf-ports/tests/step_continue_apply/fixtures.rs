use super::*;
use wf_domain::claim::Claim;
use wf_domain::workflow::{Create, reduce};

pub(super) fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    ))
}

pub(super) fn clock(now: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?)
}

pub(super) const fn log(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}

pub(super) fn limits() -> TestResult<CodecLimits> {
    Ok(CodecLimits::new(AdmissionLimits::new(32, 8192, 4)?))
}

pub(super) fn genesis() -> TestResult<ControlState> {
    Ok(plan_control(
        ControlState::initial(clock(0)?),
        EntryIdentity::new(log(0), PreparedDigest::new([0x31; 32])),
        EntryStamp::Unstamped,
    )?
    .next())
}

pub(super) fn record() -> TestResult<WorkflowRecord> {
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
            worker: b"worker".to_vec(),
            token: LeaseToken::new(b"lease".to_vec())?,
            lease_ms: 30_000,
        }),
    };
    let (_, Some(write), _) = reduce(Some(&record), &claim)?.into_parts() else {
        return Err("fixture Claim".into());
    };
    Ok(write.record)
}

pub(super) fn request(now: u64, to: &[u8], payload: Option<&[u8]>) -> TestResult<PreparedRequest> {
    Ok(PreparedRequest {
        scope: scope()?,
        request: RequestId::new(b"q".to_vec())?,
        intent: ClientIntent::new(1, [0; 32])?,
        stamp: clock(now)?,
        operation: PreparedOperation::Batch(vec![PreparedBatchItem {
            partition: PartitionId::new(b"p".to_vec())?,
            create_priority: 0,
            command: CommittedCommand {
                id: RecordId::new(b"i".to_vec())?,
                now: TimestampMillis::new(now),
                operation: Operation::StepContinue(StepContinue {
                    guard: LeaseGuard {
                        token: LeaseToken::new(b"lease".to_vec())?,
                        fence: Fence::new(1),
                    },
                    from: b"authorize".to_vec(),
                    to: to.to_vec(),
                    worker: None,
                    lease_ms: 30_000,
                    payload: payload.map(<[u8]>::to_vec),
                }),
            },
        }]),
    })
}

pub(super) fn frame(now: u64, to: &[u8], payload: Option<&[u8]>) -> TestResult<PreparedBytes> {
    Ok(encode_step_continue(
        request(now, to, payload)?,
        limits()?,
        WORKSPACE,
    )?)
}

pub(super) fn fresh(bytes: &[u8], workspace: usize) -> TestResult<FreshRecordBatch<'_>> {
    let query = scope()?;
    let answer = inspect_step_continue_point(
        prepare_step_continue(bytes, limits()?, WORKSPACE)?,
        log(1),
        PointGateView {
            scope: &query,
            control: genesis()?,
            retained_nonempty: false,
            outcome: None,
        },
        limits()?,
        workspace,
    );
    assert!(
        answer.is_ok(),
        "healthy Step inspection: {:?}",
        answer.as_ref().err()
    );
    let RecordBatchInspection::Fresh(fresh) = answer? else {
        return Err("fixture was not Fresh".into());
    };
    Ok(fresh)
}

pub(super) fn plan<'current>(
    frame: &PreparedBytes,
    record: &'current WorkflowRecord,
    mode: StateMode,
) -> TestResult<RecordBatchApplyPlan<'current>> {
    let fresh = fresh(frame.as_bytes(), WORKSPACE)?;
    super::views::with_view(record, mode, |view| {
        Ok(decide_fresh_step_continue_touched(fresh, view, 0, 100)?)
    })
}

pub(super) fn metadata(outcome: &RetainedOutcome) -> RetainedMetadata {
    RetainedMetadata {
        scope: outcome.scope().clone(),
        request: outcome.request().clone(),
        intent: outcome.intent(),
        original_position: outcome.original_position(),
        original_time: outcome.original_time(),
        expires_at: outcome.expires_at(),
        status: outcome.status(),
    }
}
