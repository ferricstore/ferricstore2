use super::{fixtures as f, *};
use wf_domain::claim::Claim;
use wf_domain::value::DomainError;
use wf_domain::workflow::{Create, WorkflowRecord, reduce};

fn running() -> TestResult<WorkflowRecord> {
    let command = CommittedCommand {
        id: RecordId::new(b"i".to_vec())?,
        now: TimestampMillis::new(0),
        operation: Operation::Create(Create {
            workflow_type: b"email".to_vec(),
            state: b"authorize".to_vec(),
            run_at: None,
            payload: Some(b"old".to_vec()),
        }),
    };
    let (_, Some(write), _) = reduce(None, &command)?.into_parts() else {
        return Err("fixture creation".into());
    };
    let record = write.record;
    let command = CommittedCommand {
        id: record.id().clone(),
        now: TimestampMillis::new(1),
        operation: Operation::Claim(Claim {
            expected_state: b"authorize".to_vec(),
            worker: b"worker".to_vec(),
            token: LeaseToken::new(b"lease".to_vec())?,
            lease_ms: 30_000,
        }),
    };
    let (_, Some(write), _) = reduce(Some(&record), &command)?.into_parts() else {
        return Err("fixture claim".into());
    };
    Ok(write.record)
}

#[test]
fn step_codec_feeds_existing_guarded_step_reduction() -> TestResult {
    f::encoded(f::request()?)?;
    let record = running()?;
    let mut request = f::request()?;
    f::step_mut(&mut request)?.guard.fence = record.fence();
    let encoded = f::encoded(request)?;
    let decoded = f::decoded(encoded.as_bytes())?;
    let PreparedOperation::Batch(items) = &decoded.request().operation else {
        return Err("decoded batch".into());
    };
    let [item] = items.as_slice() else {
        return Err("decoded singleton".into());
    };
    let plan = reduce(Some(&record), &item.command)?;
    let next = &plan.write().ok_or("Step write")?.record;
    assert_eq!(
        (next.state(), next.run_state()),
        (b"running".as_slice(), Some(b"charge".as_slice()))
    );
    assert_eq!((next.version(), next.fence().get()), (3, 2));
    assert_eq!(next.payload(), Some(b"\0\xff".as_slice()));
    assert_eq!(
        next.lease().ok_or("Step lease")?.token.as_bytes(),
        b"worker:2:2"
    );
    assert_eq!(
        reduce(Some(next), &item.command).err(),
        Some(DomainError::StaleLease)
    );
    let mut invalid = f::request()?;
    let step = f::step_mut(&mut invalid)?;
    step.guard.fence = record.fence();
    step.from.clear();
    step.to.clear();
    step.worker = Some(Vec::new());
    step.lease_ms = 0;
    step.payload = Some(Vec::new());
    let invalid = f::encoded(invalid)?;
    let decoded = f::decoded(invalid.as_bytes())?;
    let PreparedOperation::Batch(items) = &decoded.request().operation else {
        return Err("decoded batch".into());
    };
    let Operation::StepContinue(step) = &items[0].command.operation else {
        return Err("decoded Step".into());
    };
    assert!(step.from.is_empty() && step.to.is_empty());
    assert_eq!(step.worker.as_deref(), Some(b"".as_slice()));
    assert_eq!(step.lease_ms, 0);
    assert_eq!(step.payload.as_deref(), Some(b"".as_slice()));
    Ok(())
}
