//! Neutral authority and validated changes, using explicit contract doubles.
//! No backend, publication, crash recovery or durability is established here.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::policy::InstalledRetryPolicy;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::GroupView;
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::indexed::{IndexedGroupView, OrderingMetadata};
use wf_domain::workflow::request::gate::GateView;
use wf_domain::workflow::request::indexed::IndexedRequestView;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedOutcome, RetainedStatus};
use wf_domain::workflow::{CommittedCommand, Create, Operation, Terminal, reduce};
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use wf_ports::log::{
    CodecLimits, RecordBatchApplyDecision, RecordBatchApplyPlan, decide_record_batch,
    encode_record_batch,
};
use wf_ports::state::{GroupApplyChanges, StateTransactionError, StoredWorkflowRecord};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, Fence, HlcTimestamp, LogIdentity, TimestampMillis,
};

mod independent;
mod protocol;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    ))
}

fn clock(now: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?)
}

fn create(payload: Option<Vec<u8>>) -> Operation {
    Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload,
    })
}

fn missing_complete() -> TestResult<Operation> {
    Ok(Operation::Complete(Terminal {
        guard: LeaseGuard {
            token: LeaseToken::new(b"token".to_vec())?,
            fence: Fence::new(1),
        },
        payload: None,
        value: None,
    }))
}

fn sample_record(
    id: &[u8],
    workflow_type: &[u8],
    state: &[u8],
) -> TestResult<StoredWorkflowRecord> {
    let command = CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(1),
        operation: Operation::Create(Create {
            workflow_type: workflow_type.to_vec(),
            state: state.to_vec(),
            run_at: None,
            payload: None,
        }),
    };
    let record = reduce(None, &command)?
        .into_parts()
        .1
        .ok_or("create write absent")?
        .record;
    Ok(StoredWorkflowRecord {
        tenant: TenantId::new(b"t".to_vec())?,
        partition: PartitionId::new(b"p".to_vec())?,
        record,
        ordering: OrderingMetadata {
            priority: 0,
            state_enter_seq: 0,
        },
    })
}

fn with_plan<T>(
    scope: &RequestScope,
    previous: ControlState,
    at: (u64, u64),
    original: Option<&RetainedOutcome>,
    operation: Operation,
    inspect: impl FnOnce(RecordBatchApplyPlan<'_>) -> TestResult<T>,
) -> TestResult<T> {
    let partition = PartitionId::new(b"p".to_vec())?;
    let request_id = RequestId::new(b"request".to_vec())?;
    let request = PreparedRequest {
        scope: scope.clone(),
        request: request_id,
        intent: ClientIntent::new(1, [0; 32])?,
        stamp: clock(at.1)?,
        operation: PreparedOperation::Batch(vec![PreparedBatchItem {
            partition: partition.clone(),
            command: CommittedCommand {
                id: RecordId::new(b"i".to_vec())?,
                now: TimestampMillis::new(at.1),
                operation,
            },
            create_priority: 0,
        }]),
    };
    let limits = CodecLimits::new(AdmissionLimits::default());
    let frame = encode_record_batch(request, limits, 64 * 1_024 * 1_024)?;
    let placements = BTreeMap::from([(partition, scope.group().clone())]);
    let current = BTreeMap::new();
    let records = BTreeMap::new();
    let parallel = BTreeMap::new();
    let fifo = BTreeMap::new();
    let policies = BTreeMap::new();
    let retry: BTreeMap<&[u8], &InstalledRetryPolicy> = BTreeMap::new();
    let outcomes = original.map_or_else(BTreeMap::new, |value| {
        BTreeMap::from([(value.request().clone(), value.clone())])
    });
    let view = IndexedRequestView {
        gate: GateView {
            scope,
            control: previous,
            outcomes: &outcomes,
        },
        group: IndexedGroupView {
            batch: GroupView {
                group: scope.group(),
                placements: &placements,
                records: &current,
            },
            records: &records,
            parallel: &parallel,
            fifo: &fifo,
            policies: &policies,
        },
    };
    let plan = decide_record_batch(
        frame.as_bytes(),
        LogIdentity::new(1, 7, AppliedPosition::new(at.0)),
        view,
        &retry,
        1_000,
        limits,
        64 * 1_024 * 1_024,
    )?;
    drop(frame);
    inspect(plan)
}

fn original(scope: &RequestScope) -> TestResult<(ControlState, RetainedOutcome)> {
    with_plan(
        scope,
        ControlState::initial(clock(0)?),
        (0, 1),
        None,
        create(None),
        |plan| {
            let control = plan.control().next();
            let RecordBatchApplyDecision::Fresh { outcome, .. } = plan.into_parts().1 else {
                return Err("original was not Fresh".into());
            };
            Ok((control, *outcome))
        },
    )
}

#[test]
fn apply_changes_separate_business_and_control_publication() -> TestResult {
    let scope = scope()?;
    let initial = ControlState::initial(clock(0)?);
    with_plan(&scope, initial, (0, 1), None, create(None), |plan| {
        let next = plan.control().next();
        let changes = GroupApplyChanges::new(&scope, initial, plan)?;
        assert_eq!(changes.scope(), &scope);
        assert_eq!(changes.expected(), initial);
        assert_eq!(changes.control(), Some(next));
        let business = changes.business().ok_or("accepted business absent")?;
        assert_eq!(business.batch().items().len(), 1);
        let outcome = changes.outcome().ok_or("accepted outcome absent")?;
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
        assert_eq!(outcome.response(), &[0]);
        protocol::commit_after_observations(&changes)
    })?;
    with_plan(&scope, initial, (0, 1), None, missing_complete()?, |plan| {
        let changes = GroupApplyChanges::new(&scope, initial, plan)?;
        assert!(changes.business().is_none());
        assert_eq!(
            changes
                .outcome()
                .ok_or("rejected outcome absent")?
                .response(),
            &[1, 0, 0]
        );
        assert!(changes.control().is_some());
        Ok(())
    })?;
    let (processed, retained) = original(&scope)?;
    for (at, operation, expected) in [
        ((1, 2), create(None), "replay"),
        ((1, 2), create(Some(b"different".to_vec())), "conflict"),
        ((1, 1_001), create(None), "expired"),
        ((0, 1), create(None), "log"),
    ] {
        with_plan(&scope, processed, at, Some(&retained), operation, |plan| {
            let matches = matches!(
                (expected, plan.decision()),
                ("replay", RecordBatchApplyDecision::Replay(_))
                    | ("conflict", RecordBatchApplyDecision::IntentConflict)
                    | ("expired", RecordBatchApplyDecision::Expired)
                    | ("log", RecordBatchApplyDecision::LogReplay)
            );
            assert!(matches);
            let next = plan.control().next();
            let changes = GroupApplyChanges::new(&scope, processed, plan)?;
            assert!(changes.business().is_none());
            assert!(changes.outcome().is_none());
            assert_eq!(changes.control(), (expected != "log").then_some(next));
            Ok(())
        })?;
    }
    assert_eq!(
        retained.original_position().index(),
        AppliedPosition::new(0)
    );
    assert_eq!(retained.expires_at(), TimestampMillis::new(1_001));
    Ok(())
}

#[test]
fn invalid_changes_reject_foreign_scope_and_predecessor() -> TestResult {
    let scope = scope()?;
    let initial = ControlState::initial(clock(0)?);
    let foreign_scopes = [
        RequestScope::new(
            TenantId::new(b"other".to_vec())?,
            scope.group().clone(),
            scope.recovery().clone(),
        ),
        RequestScope::new(
            scope.tenant().clone(),
            GroupId::new(b"other".to_vec())?,
            scope.recovery().clone(),
        ),
        RequestScope::new(
            scope.tenant().clone(),
            scope.group().clone(),
            RecoveryId::new(b"other".to_vec())?,
        ),
    ];
    for foreign in &foreign_scopes {
        with_plan(&scope, initial, (0, 1), None, create(None), |plan| {
            assert!(matches!(
                GroupApplyChanges::new(foreign, initial, plan),
                Err(StateTransactionError::InvalidChanges)
            ));
            Ok(())
        })?;
    }
    let (processed, retained) = original(&scope)?;
    for foreign in &foreign_scopes {
        with_plan(
            &scope,
            processed,
            (1, 2),
            Some(&retained),
            create(None),
            |plan| {
                assert!(matches!(
                    GroupApplyChanges::new(foreign, processed, plan),
                    Err(StateTransactionError::InvalidChanges)
                ));
                Ok(())
            },
        )?;
    }
    for wrong in [ControlState::initial(clock(9)?), processed] {
        with_plan(&scope, initial, (0, 1), None, create(None), |plan| {
            assert!(matches!(
                GroupApplyChanges::new(&scope, wrong, plan),
                Err(StateTransactionError::InvalidChanges)
            ));
            Ok(())
        })?;
    }
    assert_eq!(retained.scope(), &scope);
    assert_eq!(retained.response(), &[0]);
    Ok(())
}
