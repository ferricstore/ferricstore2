//! Concrete fresh-only policy/index dispatch after the shared retained gate.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::policy::definition::{PolicyPatch, StateChange, StatePolicyPatch};
use wf_domain::policy::installation::{
    InstallCommand, PolicyError, PolicyLimits, decide_install, prepare_install,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::batch::{BatchItem, GroupView};
use wf_domain::workflow::control::{
    ControlState, EntryIdentity, EntryStamp, PreparedDigest, plan_control,
};
use wf_domain::workflow::indexed::{IndexedBatchItem, IndexedDecision, IndexedGroupView};
use wf_domain::workflow::request::gate::{CommittedHeader, GateView};
use wf_domain::workflow::request::indexed::{
    IndexedEnvelope, IndexedRequestView, decide_indexed_request,
};
use wf_domain::workflow::request::policy::{
    PolicyEnvelope, PolicyRequestView, decide_policy_request,
};
use wf_domain::workflow::request::{
    ClientIntent, DispatchDecision, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
};
use wf_domain::workflow::{CommittedCommand, Create, Operation};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity, TimestampMillis,
};

type TestError = Box<dyn std::error::Error>;
type TestResult = Result<(), TestError>;

fn scope() -> Result<RequestScope, TestError> {
    Ok(RequestScope::new(
        TenantId::new(b"tenant".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    ))
}
fn time(now: u64) -> Result<CommittedTime, TestError> {
    Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?)
}
fn header<'a>(
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
        identity: EntryIdentity::new(
            LogIdentity::new(1, 7, AppliedPosition::new(index)),
            PreparedDigest::new([u8::try_from(index)?; 32]),
        ),
        stamp: time(now)?,
        encoded_entry: b"private-byte-fixture",
        encoded_isolation_scope: b"scope-fixture",
    })
}
fn retained(scope: &RequestScope, request: &RequestId) -> Result<RetainedOutcome, TestError> {
    Ok(RetainedOutcome::new(
        RetainedMetadata {
            scope: scope.clone(),
            request: request.clone(),
            intent: ClientIntent::new(1, [1; 32])?,
            original_position: LogIdentity::new(1, 7, AppliedPosition::new(1)),
            original_time: TimestampMillis::new(10),
            expires_at: TimestampMillis::new(100),
            status: RetainedStatus::Accepted,
        },
        1,
        b"original-bytes".to_vec(),
        AdmissionLimits::default(),
    )?)
}

fn create() -> Result<CommittedCommand, TestError> {
    Ok(CommittedCommand {
        id: RecordId::new(b"id".to_vec())?,
        now: TimestampMillis::new(10),
        operation: Operation::Create(Create {
            workflow_type: b"job".to_vec(),
            state: b"queued".to_vec(),
            run_at: None,
            payload: None,
        }),
    })
}

#[test]
fn policy_dispatch_replays_original_before_changed_cas_and_prepared_policy() -> TestResult {
    let scope = scope()?;
    let request = RequestId::new(b"policy".to_vec())?;
    let command = InstallCommand::low_level(b"job".to_vec(), PolicyPatch::default());
    let limits = PolicyLimits::default();
    let prepared = prepare_install(None, 0, &command, limits)?.bind_bytes(
        b"snapshot",
        b"envelope".to_vec(),
        limits,
    )?;
    let outcomes = BTreeMap::new();
    let view = PolicyRequestView {
        gate: GateView {
            scope: &scope,
            control: initialized(time(0)?)?,
            outcomes: &outcomes,
        },
        existing: None,
        migration_high_water: 0,
    };
    let first = decide_policy_request(
        view,
        PolicyEnvelope {
            header: header(&scope, &request, 1, 10)?,
            command: &command,
            prepared: &prepared,
        },
        limits,
        AdmissionLimits::default(),
    )?;
    assert!(matches!(first.decision(), DispatchDecision::Fresh(Ok(_))));
    assert!(first.requires_outcome_write());
    assert_eq!(
        decide_policy_request(
            PolicyRequestView {
                migration_high_water: 3,
                ..view
            },
            PolicyEnvelope {
                header: header(&scope, &request, 1, 10)?,
                command: &command,
                prepared: &prepared
            },
            limits,
            AdmissionLimits::default()
        ),
        Err(ApplyError::InvalidAuthority)
    );
    let original = retained(&scope, &request)?;
    let outcomes = BTreeMap::from([(request.clone(), original.clone())]);
    let mut stale = command.clone();
    stale.expected_generation = Some(99);
    let view = PolicyRequestView {
        gate: GateView {
            control: first.control().next(),
            outcomes: &outcomes,
            ..view.gate
        },
        migration_high_water: 77,
        ..view
    };
    let duplicate = decide_policy_request(
        view,
        PolicyEnvelope {
            header: header(&scope, &request, 2, 20)?,
            command: &stale,
            prepared: &prepared,
        },
        limits,
        AdmissionLimits::default(),
    )?;
    assert!(
        matches!(duplicate.decision(), DispatchDecision::Replay(outcome) if *outcome == &original)
    );
    assert!(!duplicate.requires_outcome_write());
    assert_eq!(outcomes[&request], original);
    assert_eq!(duplicate.control().next().position().get(), 2);
    Ok(())
}

#[test]
fn indexed_dispatch_replay_emits_no_index_plan_or_ordinal_and_fresh_checks_time() -> TestResult {
    let scope = scope()?;
    let request = RequestId::new(b"indexed".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), scope.group().clone())]);
    let records = BTreeMap::new();
    let current = BTreeMap::new();
    let parallel = BTreeMap::new();
    let fifo = BTreeMap::new();
    let policies = BTreeMap::new();
    let outcomes = BTreeMap::new();
    let view = IndexedRequestView {
        gate: GateView {
            scope: &scope,
            control: initialized(time(0)?)?,
            outcomes: &outcomes,
        },
        group: IndexedGroupView {
            batch: GroupView {
                group: scope.group(),
                placements: &placements,
                records: &records,
            },
            records: &current,
            parallel: &parallel,
            fifo: &fifo,
            policies: &policies,
        },
    };
    let command = create()?;
    let items = [IndexedBatchItem::new(BatchItem::new(
        &partition, &command, b"key",
    ))];
    let envelope = IndexedEnvelope {
        header: header(&scope, &request, 1, 10)?,
        items: &items,
        first_ordinal: 2,
    };
    let first = decide_indexed_request(view, envelope, AdmissionLimits::default())?;
    let DispatchDecision::Fresh(IndexedDecision::Accepted(plan)) = first.decision() else {
        return Err("fresh indexed request rejected".into());
    };
    assert_eq!(plan.consumed_ordinals(), 1);
    assert_eq!(
        plan.metadata()
            .next()
            .ok_or("required test fixture value absent")?
            .state_enter_seq,
        (1_u128 << 32) + 2
    );
    assert_eq!(
        decide_indexed_request(
            view,
            IndexedEnvelope {
                header: header(&scope, &request, 1, 11)?,
                ..envelope
            },
            AdmissionLimits::default()
        ),
        Err(ApplyError::InvalidAuthority)
    );
    let applied = IndexedRequestView {
        gate: GateView {
            control: first.control().next(),
            ..view.gate
        },
        ..view
    };
    let log_replay = decide_indexed_request(applied, envelope, AdmissionLimits::default())?;
    assert!(matches!(log_replay.decision(), DispatchDecision::LogReplay));
    assert_eq!(log_replay.control().next(), applied.gate.control);
    assert!(!log_replay.requires_outcome_write());
    let original = retained(&scope, &request)?;
    let outcomes = BTreeMap::from([(request.clone(), original.clone())]);
    let duplicate = decide_indexed_request(
        IndexedRequestView {
            gate: GateView {
                outcomes: &outcomes,
                ..applied.gate
            },
            ..applied
        },
        IndexedEnvelope {
            header: header(&scope, &request, 2, 20)?,
            first_ordinal: u32::MAX,
            ..envelope
        },
        AdmissionLimits::default(),
    )?;
    assert!(
        matches!(duplicate.decision(), DispatchDecision::Replay(outcome) if *outcome == &original)
    );
    assert_eq!(outcomes[&request], original);
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

#[test]
fn fresh_policy_business_rejection_keeps_stamped_control_and_requires_original_outcome()
-> TestResult {
    let scope = scope()?;
    let request = RequestId::new(b"stale-policy".to_vec())?;
    let mut command = InstallCommand::low_level(b"job".to_vec(), PolicyPatch::default());
    let limits = PolicyLimits::default();
    let prepared = prepare_install(None, 0, &command, limits)?.bind_bytes(
        b"snapshot",
        b"envelope".to_vec(),
        limits,
    )?;
    command.expected_generation = Some(7);
    let outcomes = BTreeMap::new();
    let result = decide_policy_request(
        PolicyRequestView {
            gate: GateView {
                scope: &scope,
                control: initialized(time(0)?)?,
                outcomes: &outcomes,
            },
            existing: None,
            migration_high_water: 0,
        },
        PolicyEnvelope {
            header: header(&scope, &request, 1, 11)?,
            command: &command,
            prepared: &prepared,
        },
        limits,
        AdmissionLimits::default(),
    )?;
    assert!(
        matches!(result.decision(), DispatchDecision::Fresh(Err(error)) if error.error() == wf_domain::policy::installation::PolicyError::StaleGeneration)
    );
    assert!(result.requires_outcome_write());
    assert_eq!(result.control().next().position().get(), 1);
    assert_eq!(result.control().execution_time().get(), 11);
    assert_eq!(result.control().next().time().wall().get(), 11);
    assert!(outcomes.is_empty());
    Ok(())
}

#[test]
fn fresh_policy_stale_cas_cannot_mask_a_foreign_stored_type() -> TestResult {
    let scope = scope()?;
    let request = RequestId::new(b"foreign-stored".to_vec())?;
    let limits = PolicyLimits::default();
    let foreign = InstallCommand::low_level(b"other".to_vec(), PolicyPatch::default());
    let stored = prepare_install(None, 0, &foreign, limits)?.bind_bytes(
        b"snapshot",
        b"stored".to_vec(),
        limits,
    )?;
    let mut command = InstallCommand::low_level(b"job".to_vec(), PolicyPatch::default());
    let prepared = prepare_install(None, 0, &command, limits)?.bind_bytes(
        b"snapshot",
        b"prepared".to_vec(),
        limits,
    )?;
    command.expected_generation = Some(99);
    assert_eq!(
        decide_install(Some(&stored), 0, &command, &prepared, limits),
        Err(PolicyError::StaleGeneration)
    );
    let outcomes = BTreeMap::new();
    let control = initialized(time(0)?)?;
    assert_eq!(
        decide_policy_request(
            PolicyRequestView {
                gate: GateView {
                    scope: &scope,
                    control,
                    outcomes: &outcomes
                },
                existing: Some(&stored),
                migration_high_water: 0,
            },
            PolicyEnvelope {
                header: header(&scope, &request, 1, 11)?,
                command: &command,
                prepared: &prepared,
            },
            limits,
            AdmissionLimits::default(),
        ),
        Err(ApplyError::InvalidAuthority)
    );
    assert!(outcomes.is_empty());
    assert_eq!(control.position().get(), 0);
    assert_eq!(stored.policy().workflow_type(), b"other");
    Ok(())
}

#[test]
fn fresh_policy_stale_cas_cannot_mask_a_stored_policy_over_selected_limits() -> TestResult {
    let scope = scope()?;
    let request = RequestId::new(b"over-profile".to_vec())?;
    let wide = PolicyLimits::new(2)?;
    let narrow = PolicyLimits::new(1)?;
    let mut command = InstallCommand::low_level(
        b"job".to_vec(),
        PolicyPatch {
            states: BTreeMap::from([
                (
                    b"a".to_vec(),
                    StateChange::Patch(StatePolicyPatch::default()),
                ),
                (
                    b"b".to_vec(),
                    StateChange::Patch(StatePolicyPatch::default()),
                ),
            ]),
            ..PolicyPatch::default()
        },
    );
    let stored = prepare_install(None, 0, &command, wide)?.bind_bytes(
        b"snapshot",
        b"stored".to_vec(),
        wide,
    )?;
    let prepared = prepare_install(Some(&stored), 0, &command, wide)?.bind_bytes(
        b"snapshot",
        b"prepared".to_vec(),
        wide,
    )?;
    command.expected_generation = Some(99);
    assert_eq!(
        decide_install(Some(&stored), 0, &command, &prepared, narrow),
        Err(PolicyError::StaleGeneration)
    );
    let outcomes = BTreeMap::new();
    let control = initialized(time(0)?)?;
    assert_eq!(
        decide_policy_request(
            PolicyRequestView {
                gate: GateView {
                    scope: &scope,
                    control,
                    outcomes: &outcomes
                },
                existing: Some(&stored),
                migration_high_water: 0,
            },
            PolicyEnvelope {
                header: header(&scope, &request, 1, 11)?,
                command: &command,
                prepared: &prepared,
            },
            narrow,
            AdmissionLimits::default(),
        ),
        Err(ApplyError::InvalidAuthority)
    );
    assert!(outcomes.is_empty());
    assert_eq!(control.position().get(), 0);
    assert_eq!(stored.policy().states().len(), 2);
    Ok(())
}

#[test]
fn coherent_stored_policy_stale_cas_remains_a_retained_business_rejection() -> TestResult {
    let scope = scope()?;
    let request = RequestId::new(b"coherent-stale".to_vec())?;
    let limits = PolicyLimits::default();
    let mut command = InstallCommand::low_level(b"job".to_vec(), PolicyPatch::default());
    let stored = prepare_install(None, 0, &command, limits)?.bind_bytes(
        b"snapshot",
        b"stored".to_vec(),
        limits,
    )?;
    let prepared = prepare_install(Some(&stored), 0, &command, limits)?.bind_bytes(
        b"snapshot",
        b"prepared".to_vec(),
        limits,
    )?;
    command.expected_generation = Some(99);
    let outcomes = BTreeMap::new();
    let result = decide_policy_request(
        PolicyRequestView {
            gate: GateView {
                scope: &scope,
                control: initialized(time(0)?)?,
                outcomes: &outcomes,
            },
            existing: Some(&stored),
            migration_high_water: 0,
        },
        PolicyEnvelope {
            header: header(&scope, &request, 1, 11)?,
            command: &command,
            prepared: &prepared,
        },
        limits,
        AdmissionLimits::default(),
    )?;
    assert!(
        matches!(result.decision(), DispatchDecision::Fresh(Err(error))
        if error.error() == PolicyError::StaleGeneration)
    );
    assert!(result.requires_outcome_write());
    assert_eq!(result.control().next().position().get(), 1);
    assert_eq!(result.control().execution_time().get(), 11);
    assert_eq!(stored.generation(), 1);
    assert_eq!(stored.encoded(), b"stored");
    assert!(outcomes.is_empty());
    Ok(())
}

#[test]
fn retained_policy_replay_precedes_invalid_stored_type_profile_and_high_water() -> TestResult {
    let scope = scope()?;
    let request = RequestId::new(b"replay-invalid-view".to_vec())?;
    let limits = PolicyLimits::new(1)?;
    let foreign = InstallCommand::low_level(
        b"other".to_vec(),
        PolicyPatch {
            states: BTreeMap::from([
                (
                    b"a".to_vec(),
                    StateChange::Patch(StatePolicyPatch::default()),
                ),
                (
                    b"b".to_vec(),
                    StateChange::Patch(StatePolicyPatch::default()),
                ),
            ]),
            ..PolicyPatch::default()
        },
    );
    let stored = prepare_install(None, 0, &foreign, PolicyLimits::new(2)?)?.bind_bytes(
        b"snapshot",
        b"stored".to_vec(),
        PolicyLimits::new(2)?,
    )?;
    let mut command = InstallCommand::low_level(b"job".to_vec(), PolicyPatch::default());
    let prepared = prepare_install(None, 0, &command, limits)?.bind_bytes(
        b"snapshot",
        b"prepared".to_vec(),
        limits,
    )?;
    command.expected_generation = Some(99);
    let original = retained(&scope, &request)?;
    let outcomes = BTreeMap::from([(request.clone(), original.clone())]);
    let control = plan_control(
        initialized(time(0)?)?,
        header(&scope, &request, 1, 10)?.identity,
        EntryStamp::Stamped(time(10)?),
    )?
    .next();
    let replay = decide_policy_request(
        PolicyRequestView {
            gate: GateView {
                scope: &scope,
                control,
                outcomes: &outcomes,
            },
            existing: Some(&stored),
            migration_high_water: u64::MAX,
        },
        PolicyEnvelope {
            header: header(&scope, &request, 2, 20)?,
            command: &command,
            prepared: &prepared,
        },
        limits,
        AdmissionLimits::default(),
    )?;
    let DispatchDecision::Replay(outcome) = replay.decision() else {
        return Err("original policy result did not replay".into());
    };
    assert_eq!(*outcome, &original);
    assert_eq!(
        outcome.response().as_ptr(),
        outcomes[&request].response().as_ptr()
    );
    assert!(!replay.requires_outcome_write());
    assert_eq!(replay.control().next().position().get(), 2);
    assert_eq!(replay.control().execution_time().get(), 20);
    assert_eq!(outcomes[&request], original);
    Ok(())
}
