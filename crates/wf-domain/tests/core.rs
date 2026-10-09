//! Source-derived create/claim observations for the reduced M1 record model.

#![forbid(unsafe_code)]

use wf_domain::claim::Claim;
use wf_domain::history::EventKind;
use wf_domain::lease::LeaseToken;
use wf_domain::workflow::{CommittedCommand, Create, Operation, Outcome, WorkflowStatus, reduce};
use wf_types::identity::RecordId;
use wf_types::version::TimestampMillis;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn create_records_source_initial_counters_due_time_and_history() -> TestResult {
    // flow_create.ex:259-293: version=1, attempts=0, fence=0, unleased.
    let command = CommittedCommand {
        id: RecordId::new(b"record".to_vec())?,
        now: TimestampMillis::new(1_000),
        operation: Operation::Create(Create {
            workflow_type: b"type".to_vec(),
            state: b"queued".to_vec(),
            run_at: Some(TimestampMillis::new(1_200)),
            payload: Some(Vec::new()),
        }),
    };
    let before = command.clone();
    let plan = reduce(None, &command)?;
    assert_eq!(plan.outcome(), Outcome::Changed);
    let write = plan.write().ok_or("create requires a record write")?;
    assert_eq!(write.expected_version, None);
    let record = &write.record;
    assert_eq!(record.id(), &command.id);
    assert_eq!(record.state(), b"queued");
    assert_eq!(record.workflow_type(), b"type");
    assert_eq!(record.status(), WorkflowStatus::Scheduled);
    assert_eq!(
        (record.version(), record.attempts(), record.fence().get()),
        (1, 0, 0)
    );
    assert_eq!(record.created_at().get(), 1_000);
    assert_eq!(record.updated_at().get(), 1_000);
    assert_eq!(record.next_run_at(), Some(TimestampMillis::new(1_200)));
    assert_eq!(record.payload(), Some(b"".as_slice()));
    assert_eq!(record.result(), None);
    assert_eq!(record.error(), None);
    assert_eq!(record.lease(), None);
    assert_eq!(record.run_state(), None);
    assert_eq!(record.lease_deadline().get(), 0);
    let history = plan.history().ok_or("create requires one event")?;
    assert_eq!(history.kind, EventKind::Created);
    assert_eq!(history.event_id, "1000-1");
    assert_eq!(history.version, 1);
    assert_eq!(history.fence.get(), 0);
    assert_eq!(command, before);
    assert_eq!(reduce(None, &command)?, plan);
    let error = reduce(Some(record), &command)
        .err()
        .ok_or("duplicate create must reject")?;
    assert_eq!(error.to_string(), "ERR flow already exists");
    Ok(())
}

#[test]
fn explicit_claim_checks_due_time_and_preserves_logical_run_state() -> TestResult {
    // flow_retention_values.ex:475-508, 551-561: one selected record, due <= now.
    let id = RecordId::new(b"claim-record".to_vec())?;
    let create = CommittedCommand {
        id: id.clone(),
        now: TimestampMillis::new(1_000),
        operation: Operation::Create(Create {
            workflow_type: b"type".to_vec(),
            state: b"queued".to_vec(),
            run_at: Some(TimestampMillis::new(1_200)),
            payload: Some(b"payload".to_vec()),
        }),
    };
    let created = reduce(None, &create)?;
    let record = &created.write().ok_or("missing create write")?.record;
    let before = record.clone();
    let mut claim = CommittedCommand {
        id,
        now: TimestampMillis::new(1_199),
        operation: Operation::Claim(Claim {
            expected_state: b"queued".to_vec(),
            worker: b"worker".to_vec(),
            token: LeaseToken::new(vec![0, 255, b':'])?,
            lease_ms: 30_000,
        }),
    };
    let early = reduce(Some(record), &claim)?;
    assert_eq!(early.outcome(), Outcome::NotEligible);
    assert_eq!(early.write(), None);
    assert_eq!(early.history(), None);
    claim.now = TimestampMillis::new(1_200);
    let plan = reduce(Some(record), &claim)?;
    let write = plan.write().ok_or("due claim needs a write")?;
    assert_eq!(write.expected_version, Some(1));
    let running = &write.record;
    assert_eq!(running.state(), b"running");
    assert_eq!(running.run_state(), Some(b"queued".as_slice()));
    assert_eq!(running.status(), WorkflowStatus::Running);
    assert_eq!(
        (running.version(), running.fence().get(), running.attempts()),
        (2, 1, 0)
    );
    assert_eq!(running.lease_deadline().get(), 31_200);
    assert_eq!(running.next_run_at(), Some(TimestampMillis::new(31_200)));
    let lease = running.lease().ok_or("claim must own a lease")?;
    assert_eq!(lease.worker, b"worker");
    assert_eq!(lease.token.as_bytes(), &[0, 255, b':']);
    assert_eq!(running.payload(), Some(b"payload".as_slice()));
    assert_eq!(
        plan.history().ok_or("claim needs one event")?.kind,
        EventKind::Claimed
    );
    assert_eq!(record, &before);
    assert_eq!(reduce(Some(record), &claim)?, plan);
    Ok(())
}
