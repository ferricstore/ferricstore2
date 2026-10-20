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
    CommittedHeader, GateDecision, GateView, PointGateView, inspect_point_request, inspect_request,
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

fn point_outcome(
    scope: RequestScope,
    request: RequestId,
    index: u64,
    original_time: u64,
) -> Result<RetainedOutcome, TestError> {
    Ok(RetainedOutcome::new(
        RetainedMetadata {
            scope,
            request,
            intent: ClientIntent::new(1, [1; 32])?,
            original_position: identity(index).log(),
            original_time: TimestampMillis::new(original_time),
            expires_at: TimestampMillis::new(100),
            status: RetainedStatus::Accepted,
        },
        1,
        vec![0],
        AdmissionLimits::default(),
    )?)
}

fn point_header<'a>(
    scope: &'a RequestScope,
    request: &'a RequestId,
    index: u64,
    now: u64,
) -> Result<CommittedHeader<'a>, TestError> {
    Ok(CommittedHeader {
        format_version: 1,
        scope,
        request,
        intent: ClientIntent::new(1, [1; 32])?,
        identity: identity(index),
        stamp: time(now, now)?,
        encoded_entry: b"entry",
        encoded_isolation_scope: b"scope",
    })
}

#[test]
fn point_gate_matches_complete_view_classifications() -> TestResult {
    let original_scope = scope(b"tenant")?;
    let request = RequestId::new(b"request".to_vec())?;
    let original = point_outcome(original_scope.clone(), request.clone(), 1, 10)?;
    let control = plan_control(
        initialized(time(0, 0)?)?,
        identity(1),
        EntryStamp::Stamped(time(10, 10)?),
    )?
    .next();
    for class in 0..6 {
        let query = original_scope.clone();
        let mut header = point_header(
            &original_scope,
            &request,
            if class == 4 { 1 } else { 2 },
            match class {
                3 => 100,
                4 => 10,
                _ => 20,
            },
        )?;
        if class == 2 {
            header.intent = ClientIntent::new(1, [2; 32])?;
        }
        let retained = (class != 0 && class != 5).then_some(&original);
        let outcomes = if class == 0 {
            BTreeMap::new()
        } else if class == 5 {
            let other = RequestId::new(b"other".to_vec())?;
            BTreeMap::from([(other.clone(), point_outcome(query.clone(), other, 1, 10)?)])
        } else {
            BTreeMap::from([(request.clone(), original.clone())])
        };
        let point = inspect_point_request(
            PointGateView {
                scope: &query,
                control,
                retained_nonempty: class != 0,
                outcome: retained,
            },
            header,
            AdmissionLimits::default(),
        )?;
        let complete = inspect_request(
            GateView {
                scope: &original_scope,
                control,
                outcomes: &outcomes,
            },
            header,
            AdmissionLimits::default(),
        )?;
        drop(query);
        assert_eq!(point.control(), complete.control());
        assert_eq!(point.decision(), complete.decision());
        match class {
            0 | 5 => assert_eq!(point.decision(), GateDecision::Fresh),
            1 => {
                let GateDecision::Replay(replayed) = point.decision() else {
                    return Err("expected original replay".into());
                };
                assert_eq!(std::ptr::from_ref(replayed), std::ptr::from_ref(&original));
                assert_eq!(replayed.response(), [0]);
                assert_eq!(replayed.original_position(), identity(1).log());
                assert_eq!(replayed.original_time().get(), 10);
                assert_eq!(replayed.expires_at().get(), 100);
            }
            2 => assert_eq!(point.decision(), GateDecision::IntentConflict),
            3 => assert_eq!(point.decision(), GateDecision::Expired),
            4 => assert_eq!(point.decision(), GateDecision::LogReplay),
            _ => return Err("unexpected class".into()),
        }
        assert_eq!(
            point.control().next().position().get(),
            if class == 4 { 1 } else { 2 }
        );
        assert_eq!(point.control().is_log_replay(), class == 4);
    }
    Ok(())
}

#[test]
fn point_gate_rejects_incoherent_cohort_and_foreign_retained() -> TestResult {
    let query = scope(b"tenant")?;
    let request = RequestId::new(b"request".to_vec())?;
    let original = point_outcome(query.clone(), request.clone(), 1, 10)?;
    let control = plan_control(
        initialized(time(0, 0)?)?,
        identity(1),
        EntryStamp::Stamped(time(10, 10)?),
    )?
    .next();
    let header = point_header(&query, &request, 2, 20)?;
    let view = PointGateView {
        scope: &query,
        control,
        retained_nonempty: true,
        outcome: None,
    };
    let invalid = Err(ApplyError::InvalidAuthority);
    assert_eq!(
        inspect_point_request(
            PointGateView {
                control: ControlState::initial(time(0, 0)?),
                ..view
            },
            header,
            AdmissionLimits::default(),
        ),
        invalid
    );
    for (index, now) in [(0, 20), (1, 10)] {
        assert_eq!(
            inspect_point_request(
                PointGateView {
                    retained_nonempty: false,
                    outcome: Some(&original),
                    ..view
                },
                point_header(&query, &request, index, now)?,
                AdmissionLimits::default(),
            ),
            invalid
        );
    }
    let fresh = inspect_point_request(
        PointGateView {
            control: ControlState::initial(time(0, 0)?),
            retained_nonempty: false,
            ..view
        },
        point_header(&query, &request, 0, 1)?,
        AdmissionLimits::default(),
    )?;
    assert_eq!(fresh.decision(), GateDecision::Fresh);
    for bad in [
        point_outcome(scope(b"foreign")?, request.clone(), 1, 10)?,
        point_outcome(query.clone(), RequestId::new(b"other".to_vec())?, 1, 10)?,
        point_outcome(query.clone(), request.clone(), 2, 10)?,
        point_outcome(query.clone(), request.clone(), 1, 11)?,
    ] {
        let bad_view = PointGateView {
            outcome: Some(&bad),
            ..view
        };
        assert_eq!(
            inspect_point_request(bad_view, header, AdmissionLimits::default()),
            invalid
        );
        let replay = inspect_point_request(
            bad_view,
            point_header(&query, &request, 1, 10)?,
            AdmissionLimits::default(),
        )?;
        assert_eq!(replay.decision(), GateDecision::LogReplay);
        assert_eq!(replay.control().next(), control);
    }
    let foreign = scope(b"foreign")?;
    assert_eq!(
        inspect_point_request(
            view,
            CommittedHeader {
                scope: &foreign,
                ..point_header(&query, &request, 0, 20)?
            },
            AdmissionLimits::default(),
        ),
        invalid
    );
    Ok(())
}

mod charged;
