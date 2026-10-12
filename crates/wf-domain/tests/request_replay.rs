//! Explicit request replay remains distinct from legacy fence duplicate behavior.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::{ApplyError, BatchDecision, BatchItem, BatchRejection, GroupView};
use wf_domain::workflow::control::{
    ControlState, EntryIdentity, EntryStamp, PreparedDigest, plan_control,
};
use wf_domain::workflow::request::{
    ClientIntent, CommittedEnvelope, RequestDecision, RequestPlan, RequestScope, RequestView,
    RetainedMetadata, RetainedOutcome, RetainedStatus, decide_request,
};
use wf_domain::workflow::{CommittedCommand, Create, DomainError, Operation};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity, TimestampMillis,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn make_scope(group: &[u8], recovery: &[u8]) -> Result<RequestScope, Box<dyn std::error::Error>> {
    Ok(RequestScope::new(
        TenantId::new(b"tenant".to_vec())?,
        GroupId::new(group.to_vec())?,
        RecoveryId::new(recovery.to_vec())?,
    ))
}

fn time(wall: u64) -> Result<CommittedTime, Box<dyn std::error::Error>> {
    Ok(CommittedTime::new(HlcTimestamp::new(wall, 0)?, wall)?)
}

fn identity(index: u64, byte: u8) -> EntryIdentity {
    EntryIdentity::new(
        LogIdentity::new(1, 7, AppliedPosition::new(index)),
        PreparedDigest::new([byte; 32]),
    )
}

fn create(now: u64) -> Result<CommittedCommand, Box<dyn std::error::Error>> {
    Ok(CommittedCommand {
        id: RecordId::new(b"id".to_vec())?,
        now: TimestampMillis::new(now),
        operation: Operation::Create(Create {
            workflow_type: b"type".to_vec(),
            state: b"queued".to_vec(),
            run_at: None,
            payload: Some(vec![]),
        }),
    })
}

fn assert_original_replay(plan: &RequestPlan<'_>, original: &RetainedOutcome) -> TestResult {
    let RequestDecision::Replay(retained) = plan.decision() else {
        return Err("original was not replayed".into());
    };
    assert_eq!(*retained, original);
    assert_eq!(retained.response(), b"exact\0original\xffresponse");
    assert_eq!(retained.original_time().get(), 10);
    assert_eq!(retained.original_position().index().get(), 1);
    assert_eq!(retained.expires_at().get(), 1_000);
    assert_eq!(plan.control().next().position().get(), 2);
    assert_eq!(plan.control().execution_time().get(), 20);
    assert!(!plan.requires_outcome_write());
    Ok(())
}

fn original_outcome(
    scope: &RequestScope,
    request: &RequestId,
    intent: ClientIntent,
    status: RetainedStatus,
) -> Result<RetainedOutcome, Box<dyn std::error::Error>> {
    Ok(RetainedOutcome::new(
        RetainedMetadata {
            scope: scope.clone(),
            request: request.clone(),
            intent,
            original_position: identity(1, 1).log(),
            original_time: TimestampMillis::new(10),
            expires_at: TimestampMillis::new(1_000),
            status,
        },
        1,
        b"exact\0original\xffresponse".to_vec(),
        AdmissionLimits::default(),
    )?)
}

#[test]
fn accepted_and_rejected_requests_replay_exact_original_before_any_changed_guard() -> TestResult {
    let scope = make_scope(b"group", b"recovery")?;
    let request = RequestId::new(b"request".to_vec())?;
    let intent = ClientIntent::new(1, [1; 32])?;
    let partition = PartitionId::new(b"partition".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), scope.group().clone())]);
    let records = BTreeMap::new();
    let outcomes = BTreeMap::new();
    let command = create(10)?;
    let items = [BatchItem::new(&partition, &command, b"key")];
    let initial = initialized(time(0)?)?;
    let envelope = CommittedEnvelope {
        format_version: 1,
        scope: &scope,
        request: &request,
        intent,
        identity: identity(1, 1),
        stamp: time(10)?,
        encoded_isolation_scope: b"scope",
        encoded_entry: b"entry-one",
        items: &items,
    };
    let first = decide_request(
        RequestView {
            scope: &scope,
            control: initial,
            group: GroupView {
                group: scope.group(),
                records: &records,
                placements: &placements,
            },
            outcomes: &outcomes,
        },
        envelope,
        AdmissionLimits::default(),
    )?;
    assert!(matches!(
        first.decision(),
        RequestDecision::Fresh(BatchDecision::Accepted(_))
    ));
    assert!(first.requires_outcome_write());
    let control = first.control().next();
    let scopes = [&scope];
    for stored_status in [
        RetainedStatus::Accepted,
        RetainedStatus::Rejected(BatchRejection::Domain(DomainError::StaleLease)),
    ] {
        let original = original_outcome(&scope, &request, intent, stored_status)?;
        let outcomes = BTreeMap::from([(request.clone(), original.clone())]);
        // No placement/current-record authority is needed to replay the original request.
        let placements = BTreeMap::new();
        let mut duplicate = envelope;
        duplicate.identity = identity(2, 2);
        duplicate.stamp = time(20)?;
        duplicate.encoded_entry = b"new-time-and-generated-inputs";
        let replay = decide_request(
            RequestView {
                scope: scopes[0],
                control,
                group: GroupView {
                    group: scope.group(),
                    records: &records,
                    placements: &placements,
                },
                outcomes: &outcomes,
            },
            duplicate,
            AdmissionLimits::default(),
        )?;
        assert_original_replay(&replay, &original)?;
        assert_eq!(outcomes[&request], original);

        duplicate.intent = ClientIntent::new(1, [2; 32])?;
        let conflict = decide_request(
            RequestView {
                scope: &scope,
                control,
                group: GroupView {
                    group: scope.group(),
                    records: &records,
                    placements: &placements,
                },
                outcomes: &outcomes,
            },
            duplicate,
            AdmissionLimits::default(),
        )?;
        assert!(matches!(
            conflict.decision(),
            RequestDecision::IntentConflict
        ));
        assert!(!conflict.requires_outcome_write());
        assert_eq!(outcomes[&request], original);
    }
    Ok(())
}

#[test]
fn scope_format_expiry_effective_time_and_exact_log_replay_are_separate() -> TestResult {
    let scope = make_scope(b"group", b"recovery")?;
    let foreign = make_scope(b"other-group", b"recovery")?;
    let restored = make_scope(b"group", b"other-recovery")?;
    let request = RequestId::new(b"request".to_vec())?;
    let intent = ClientIntent::new(1, [1; 32])?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), scope.group().clone())]);
    let records = BTreeMap::new();
    let outcomes = BTreeMap::new();
    let command = create(10)?;
    let items = [BatchItem::new(&partition, &command, b"key")];
    let initial = initialized(time(10)?)?;
    let view = RequestView {
        scope: &scope,
        control: initial,
        group: GroupView {
            group: scope.group(),
            records: &records,
            placements: &placements,
        },
        outcomes: &outcomes,
    };
    let envelope = CommittedEnvelope {
        format_version: 1,
        scope: &scope,
        request: &request,
        intent,
        identity: identity(1, 1),
        stamp: time(10)?,
        encoded_isolation_scope: b"scope",
        encoded_entry: b"bytes",
        items: &items,
    };
    for invalid_scope in [&foreign, &restored] {
        let mut invalid = envelope;
        invalid.scope = invalid_scope;
        assert_eq!(
            decide_request(view, invalid, AdmissionLimits::default()),
            Err(ApplyError::InvalidAuthority)
        );
    }
    let mut invalid = envelope;
    invalid.format_version = 2;
    assert_eq!(
        decide_request(view, invalid, AdmissionLimits::default()),
        Err(ApplyError::InvalidAuthority)
    );
    let wrong_time = create(11)?;
    let wrong_items = [BatchItem::new(&partition, &wrong_time, b"key")];
    invalid = envelope;
    invalid.items = &wrong_items;
    assert_eq!(
        decide_request(view, invalid, AdmissionLimits::default()),
        Err(ApplyError::InvalidAuthority)
    );
    let first = decide_request(view, envelope, AdmissionLimits::default())?;
    let applied = RequestView {
        control: first.control().next(),
        ..view
    };
    let log_replay = decide_request(applied, envelope, AdmissionLimits::default())?;
    assert!(matches!(log_replay.decision(), RequestDecision::LogReplay));
    assert_eq!(log_replay.control().next(), applied.control);
    assert!(!log_replay.requires_outcome_write());

    let retained = RetainedOutcome::new(
        RetainedMetadata {
            scope: scope.clone(),
            request: request.clone(),
            intent,
            original_position: envelope.identity.log(),
            original_time: TimestampMillis::new(10),
            expires_at: TimestampMillis::new(20),
            status: RetainedStatus::Accepted,
        },
        1,
        b"retained".to_vec(),
        AdmissionLimits::default(),
    )?;
    let outcomes = BTreeMap::from([(request.clone(), retained.clone())]);
    let mut next = envelope;
    next.identity = identity(2, 2);
    next.stamp = time(20)?;
    let expired = decide_request(
        RequestView {
            outcomes: &outcomes,
            ..applied
        },
        next,
        AdmissionLimits::default(),
    )?;
    assert!(matches!(expired.decision(), RequestDecision::Expired));
    assert!(!expired.requires_outcome_write());
    assert_eq!(outcomes[&request], retained);
    assert!(ClientIntent::new(2, [1; 32]).is_err());
    Ok(())
}

fn initialized(time: CommittedTime) -> Result<ControlState, Box<dyn std::error::Error>> {
    Ok(plan_control(
        ControlState::initial(time),
        EntryIdentity::new(
            LogIdentity::new(0, 0, AppliedPosition::new(0)),
            PreparedDigest::new([0; 32]),
        ),
        EntryStamp::Unstamped,
    )?
    .next())
}
