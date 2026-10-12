//! Real fresh business rejection followed by exact original retained replay.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::{BatchDecision, BatchItem, BatchRejection, GroupView};
use wf_domain::workflow::control::{
    ControlState, EntryIdentity, EntryStamp, PreparedDigest, plan_control,
};
use wf_domain::workflow::request::{
    ClientIntent, CommittedEnvelope, RequestDecision, RequestScope, RequestView, RetainedMetadata,
    RetainedOutcome, RetainedStatus, decide_request,
};
use wf_domain::workflow::{CommittedCommand, DomainError, Operation};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, Fence, HlcTimestamp, LogIdentity, TimestampMillis,
};

type TestError = Box<dyn std::error::Error>;
type TestResult = Result<(), TestError>;

fn time(physical: u64, wall: u64) -> Result<CommittedTime, TestError> {
    Ok(CommittedTime::new(HlcTimestamp::new(physical, 0)?, wall)?)
}
fn identity(index: u64) -> EntryIdentity {
    EntryIdentity::new(
        LogIdentity::new(1, 7, AppliedPosition::new(index)),
        PreparedDigest::new([0; 32]),
    )
}
fn initialized() -> Result<ControlState, TestError> {
    Ok(plan_control(
        ControlState::initial(time(0, 0)?),
        identity(0),
        EntryStamp::Unstamped,
    )?
    .next())
}

#[test]
fn fresh_record_rejection_advances_hlc_and_replays_original_before_changed_guards() -> TestResult {
    let scope = RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    );
    let request = RequestId::new(b"missing-record".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), scope.group().clone())]);
    let records = BTreeMap::new();
    let outcomes = BTreeMap::new();
    let view = RequestView {
        scope: &scope,
        control: initialized()?,
        group: GroupView {
            group: scope.group(),
            placements: &placements,
            records: &records,
        },
        outcomes: &outcomes,
    };
    let command = CommittedCommand {
        id: RecordId::new(b"absent".to_vec())?,
        now: TimestampMillis::new(11),
        operation: Operation::Retry(Retry {
            guard: LeaseGuard {
                token: LeaseToken::new(b"token".to_vec())?,
                fence: Fence::new(0),
            },
            run_at: RetryTime::Explicit(TimestampMillis::new(12)),
            payload: None,
            error: None,
        }),
    };
    let items = [BatchItem::new(&partition, &command, b"key")];
    let envelope = CommittedEnvelope {
        format_version: 1,
        scope: &scope,
        request: &request,
        intent: ClientIntent::new(1, [1; 32])?,
        identity: identity(1),
        stamp: time(11, 10)?,
        encoded_entry: b"fixture-entry",
        encoded_isolation_scope: b"scope",
        items: &items,
    };
    let first = decide_request(view, envelope, AdmissionLimits::default())?;
    let rejection = BatchRejection::Domain(DomainError::NotFound);
    assert_eq!(
        first.decision(),
        &RequestDecision::Fresh(BatchDecision::Rejected(rejection))
    );
    assert!(first.requires_outcome_write());
    assert_eq!(first.control().next().position().get(), 1);
    assert_eq!(first.control().execution_time().get(), 11);
    assert_eq!(first.control().next().time().wall().get(), 10);
    assert!(records.is_empty() && outcomes.is_empty());
    let original = RetainedOutcome::new(
        RetainedMetadata {
            scope: scope.clone(),
            request: request.clone(),
            intent: envelope.intent,
            original_position: envelope.identity.log(),
            original_time: TimestampMillis::new(11),
            expires_at: TimestampMillis::new(100),
            status: RetainedStatus::Rejected(rejection),
        },
        1,
        b"ERR flow not found".to_vec(),
        AdmissionLimits::default(),
    )?;
    let outcomes = BTreeMap::from([(request.clone(), original.clone())]);
    let missing_placements = BTreeMap::new();
    let duplicate = decide_request(
        RequestView {
            control: first.control().next(),
            outcomes: &outcomes,
            group: GroupView {
                placements: &missing_placements,
                ..view.group
            },
            ..view
        },
        CommittedEnvelope {
            identity: identity(2),
            stamp: time(20, 20)?,
            ..envelope
        },
        AdmissionLimits::default(),
    )?;
    assert_eq!(
        duplicate.decision(),
        &RequestDecision::Replay(&outcomes[&request])
    );
    assert!(!duplicate.requires_outcome_write());
    assert_eq!(duplicate.control().next().position().get(), 2);
    assert_eq!(duplicate.control().execution_time().get(), 20);
    assert_eq!(outcomes[&request], original);
    assert_eq!(outcomes[&request].response(), b"ERR flow not found");
    Ok(())
}
