//! Header-only replay is evaluated before policy/index/current guard dispatch.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::policy::definition::PolicyPatch;
use wf_domain::policy::installation::{InstallCommand, PolicyError, PolicyLimits, prepare_install};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::{
    ControlState, EntryIdentity, EntryStamp, PreparedDigest, plan_control,
};
use wf_domain::workflow::request::gate::{
    CommittedHeader, GateDecision, GateView, inspect_request,
};
use wf_domain::workflow::request::{
    ClientIntent, PolicyRejection, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity, TimestampMillis,
};

type TestError = Box<dyn std::error::Error>;
type TestResult = Result<(), TestError>;

fn scope(tenant: &[u8]) -> Result<RequestScope, TestError> {
    Ok(RequestScope::new(
        TenantId::new(tenant.to_vec())?,
        GroupId::new(b"group".to_vec())?,
        RecoveryId::new(b"recovery".to_vec())?,
    ))
}
fn time(physical: u64, wall: u64) -> Result<CommittedTime, TestError> {
    Ok(CommittedTime::new(HlcTimestamp::new(physical, 0)?, wall)?)
}
fn identity(index: u64) -> EntryIdentity {
    EntryIdentity::new(
        LogIdentity::new(1, 7, AppliedPosition::new(index)),
        PreparedDigest::new([0; 32]),
    )
}

#[test]
fn actual_encoded_scope_has_separate_inclusive_eight_kib_bound_and_tenant_equality() -> TestResult {
    let scope = scope(b"tenant")?;
    let foreign = self::scope(b"other-tenant")?;
    let request = RequestId::new(b"request".to_vec())?;
    let outcomes = BTreeMap::new();
    let view = GateView {
        scope: &scope,
        control: initialized(time(0, 0)?)?,
        outcomes: &outcomes,
    };
    let exact = vec![0; 8_192];
    let header = CommittedHeader {
        format_version: 1,
        scope: &scope,
        request: &request,
        intent: ClientIntent::new(1, [1; 32])?,
        identity: identity(1),
        stamp: time(11, 10)?,
        encoded_entry: b"entry",
        encoded_isolation_scope: &exact,
    };
    let plan = inspect_request(view, header, AdmissionLimits::default())?;
    assert_eq!(plan.decision(), GateDecision::Fresh);
    assert_eq!(plan.control().execution_time().get(), 11);
    assert_eq!(plan.control().next().time().wall().get(), 10);
    let oversized = vec![0; 8_193];
    assert_eq!(
        inspect_request(
            view,
            CommittedHeader {
                encoded_isolation_scope: &oversized,
                ..header
            },
            AdmissionLimits::default()
        ),
        Err(ApplyError::InvalidAuthority)
    );
    assert_eq!(
        inspect_request(
            view,
            CommittedHeader {
                scope: &foreign,
                ..header
            },
            AdmissionLimits::default()
        ),
        Err(ApplyError::InvalidAuthority)
    );
    Ok(())
}

#[test]
fn original_policy_failure_replays_before_new_generation_guard_without_renewing() -> TestResult {
    let scope = scope(b"tenant")?;
    let request = RequestId::new(b"request".to_vec())?;
    let intent = ClientIntent::new(1, [1; 32])?;
    let control = plan_control(
        initialized(time(0, 0)?)?,
        identity(1),
        EntryStamp::Stamped(time(10, 10)?),
    )?
    .next();
    let original = RetainedOutcome::new(
        RetainedMetadata {
            scope: scope.clone(),
            request: request.clone(),
            intent,
            original_position: identity(1).log(),
            original_time: TimestampMillis::new(10),
            expires_at: TimestampMillis::new(100),
            status: RetainedStatus::PolicyRejected(PolicyRejection::new(
                PolicyError::StaleGeneration,
            )?),
        },
        1,
        b"original-policy-error".to_vec(),
        AdmissionLimits::default(),
    )?;
    let outcomes = BTreeMap::from([(request.clone(), original.clone())]);
    let header = CommittedHeader {
        format_version: 1,
        scope: &scope,
        request: &request,
        intent,
        identity: identity(2),
        stamp: time(20, 15)?,
        encoded_entry: b"new-prepared-inputs",
        encoded_isolation_scope: b"scope",
    };
    let view = GateView {
        scope: &scope,
        control,
        outcomes: &outcomes,
    };
    let replay = inspect_request(view, header, AdmissionLimits::default())?;
    assert_eq!(replay.decision(), GateDecision::Replay(&outcomes[&request]));
    assert_eq!(replay.control().next().position().get(), 2);
    assert_eq!(outcomes[&request], original);
    // Demonstrate the changed current policy guard, which fresh dispatch would hit.
    let mut command = InstallCommand::low_level(b"type".to_vec(), PolicyPatch::default());
    command.expected_generation = Some(7);
    assert_eq!(
        prepare_install(None, 0, &command, PolicyLimits::default()),
        Err(PolicyError::StaleGeneration)
    );
    assert!(PolicyRejection::new(PolicyError::InvalidAuthority).is_err());
    assert!(PolicyRejection::new(PolicyError::PreparedMismatch).is_err());
    let applied = GateView {
        control: replay.control().next(),
        ..view
    };
    let log_replay = inspect_request(applied, header, AdmissionLimits::default())?;
    assert_eq!(log_replay.decision(), GateDecision::LogReplay);
    assert_eq!(log_replay.control().next(), applied.control);
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
