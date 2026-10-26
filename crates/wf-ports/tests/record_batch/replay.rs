//! Genuine retained results precede current workflow/index/guard execution.

use std::collections::BTreeMap;
use std::mem::size_of;

use wf_domain::claim::{Claim, ClaimRecord, DueOrder, ScopedRecordId};
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::policy::{InstalledRetryPolicy, RetryOverride};
use wf_domain::retry::{JitterSample, PolicyRetry};
use wf_domain::workflow::batch::{ApplyError, GroupView};
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::indexed::{IndexRejection, IndexedDecision, IndexedGroupView};
use wf_domain::workflow::request::gate::GateView;
use wf_domain::workflow::request::indexed::IndexedRequestView;
use wf_domain::workflow::request::{RequestValueError, RetainedOutcome, RetainedStatus};
use wf_domain::workflow::{CommittedCommand, Operation, WorkflowRecord, reduce};
use wf_ports::blob::RecordBatchValueEffects;
use wf_ports::consensus::PreparedOperation;
use wf_ports::log::{
    RecordBatchApplyDecision, RecordBatchApplyError, decide_record_batch, encode_record_batch,
};
use wf_types::version::TimestampMillis;

use super::{TestResult, WORKSPACE, clock, create, limits, log, request, scope, with_empty};

fn original(rejected: bool) -> TestResult<(ControlState, RetainedOutcome)> {
    let mut item = create(b"i")?;
    if rejected {
        item.create_priority = 255;
    }
    let frame = encode_record_batch(request(vec![item])?, limits(), WORKSPACE)?;
    let mut retained = None;
    with_empty(frame.as_bytes(), limits(), WORKSPACE, |result| {
        let plan: super::RecordBatchApplyPlan<'_> = result?;
        let (control, decision) = plan.into_parts();
        let RecordBatchApplyDecision::Fresh { operation, outcome } = decision else {
            return Err("original result is not fresh".into());
        };
        if rejected {
            assert_eq!(
                operation,
                IndexedDecision::Rejected(IndexRejection::InvalidPriority)
            );
            assert_eq!(
                outcome.status(),
                RetainedStatus::IndexedRejected(IndexRejection::InvalidPriority)
            );
            assert_eq!(outcome.response(), [1, 4, 0]);
        } else {
            assert!(matches!(operation, IndexedDecision::Accepted(_)));
            assert_eq!(outcome.response(), [0]);
        }
        retained = Some((control.next(), *outcome));
        Ok(())
    })?;
    drop(frame);
    let result = retained.ok_or("outcome absent")?;
    assert_eq!(result.1.original_time(), TimestampMillis::new(1));
    assert_eq!(result.1.original_position(), log(0));
    assert_eq!(result.1.expires_at(), TimestampMillis::new(101));
    Ok(result)
}

fn assert_exact_replay(
    control: ControlState,
    outcome: RetainedOutcome,
    rejected: bool,
) -> TestResult {
    let scope = scope()?;
    let outcomes = BTreeMap::from([(outcome.request().clone(), outcome)]);
    let original = outcomes.values().next().ok_or("outcome absent")?;
    let mut changed = create(b"i")?;
    let Operation::Create(value) = &mut changed.command.operation else {
        return Err("not create".into());
    };
    value.workflow_type = b"other-type".to_vec();
    value.payload = Some(b"changed current payload".to_vec());
    let current_record = reduce(None, &changed.command)?
        .into_parts()
        .1
        .ok_or("record absent")?
        .record;
    let current_entry = ClaimRecord {
        record: &current_record,
        partition_key: b"p",
        priority: 2,
        state_enter_seq: 99,
    };
    let current = BTreeMap::from([(ScopedRecordId::for_record(&current_entry), &current_record)]);
    // Fresh would reject this existing ID/missing placement, and these indexes are incomplete.
    let placements = BTreeMap::new();
    let records = BTreeMap::new();
    let parallel = BTreeMap::new();
    let fifo = BTreeMap::new();
    let policies = BTreeMap::new();
    let retries: BTreeMap<&[u8], &InstalledRetryPolicy> = BTreeMap::new();
    let view = IndexedRequestView {
        gate: GateView {
            scope: &scope,
            control,
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
    let mut item = create(b"i")?;
    item.create_priority = if rejected { 255 } else { 0 };
    // These generated values deliberately disagree; only Fresh may bind command.now.
    item.command.now = TimestampMillis::new(999);
    let mut duplicate = request(vec![item])?;
    duplicate.stamp = clock(5)?;
    let encoded = encode_record_batch(duplicate, limits(), WORKSPACE)?;
    let plan = decide_record_batch(
        encoded.as_bytes(),
        log(1),
        view,
        &retries,
        0,
        limits(),
        16_780_748
            + size_of::<Option<RecordBatchValueEffects>>()
            + std::mem::size_of::<wf_ports::log::DecodedRequest<'static>>(),
    )?;
    drop(encoded);
    let RecordBatchApplyDecision::Replay(replayed) = plan.decision() else {
        return Err("duplicate executed fresh".into());
    };
    assert_eq!(std::ptr::from_ref(*replayed), std::ptr::from_ref(original));
    assert_eq!(
        replayed.response(),
        if rejected { &[1, 4, 0][..] } else { &[0][..] }
    );
    assert_eq!(replayed.status(), original.status());
    assert_eq!(replayed.original_time(), TimestampMillis::new(1));
    assert_eq!(replayed.original_position(), log(0));
    assert_eq!(replayed.expires_at(), TimestampMillis::new(101));
    assert_eq!(replayed.response_version(), 1);
    assert_eq!(plan.control().execution_time(), TimestampMillis::new(5));
    assert_eq!(plan.control().next().position(), log(1).index());
    assert_eq!(current_record.workflow_type(), b"other-type");
    assert_eq!(
        current_record.payload(),
        Some(b"changed current payload".as_slice())
    );
    Ok(())
}

#[test]
fn genuine_accepted_and_rejected_replay_borrow_original_outcome() -> TestResult {
    for rejected in [false, true] {
        let (control, outcome) = original(rejected)?;
        assert_exact_replay(control, outcome, rejected)?;
    }
    Ok(())
}

fn assert_gate_branches(control: ControlState, outcome: RetainedOutcome) -> TestResult {
    let scope = scope()?;
    let outcomes = BTreeMap::from([(outcome.request().clone(), outcome)]);
    let placements = BTreeMap::new();
    let current = BTreeMap::new();
    let records = BTreeMap::new();
    let parallel = BTreeMap::new();
    let fifo = BTreeMap::new();
    let policies = BTreeMap::new();
    let retries: BTreeMap<&[u8], &InstalledRetryPolicy> = BTreeMap::new();
    let view = IndexedRequestView {
        gate: GateView {
            scope: &scope,
            control,
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
    let original = encode_record_batch(request(vec![create(b"i")?])?, limits(), WORKSPACE)?;
    let replay = decide_record_batch(
        original.as_bytes(),
        log(0),
        view,
        &retries,
        0,
        limits(),
        16_780_748
            + size_of::<Option<RecordBatchValueEffects>>()
            + std::mem::size_of::<wf_ports::log::DecodedRequest<'static>>(),
    )?;
    assert!(matches!(
        replay.decision(),
        RecordBatchApplyDecision::LogReplay
    ));
    assert_eq!(replay.control().next(), control);
    assert!(replay.control().is_log_replay());
    let mut conflict = request(vec![create(b"i")?])?;
    conflict.stamp = clock(2)?;
    if let PreparedOperation::Batch(items) = &mut conflict.operation {
        let Operation::Create(value) = &mut items[0].command.operation else {
            return Err("not create".into());
        };
        value.payload = Some(Vec::new());
    }
    let conflict = encode_record_batch(conflict, limits(), WORKSPACE)?;
    let plan = decide_record_batch(
        conflict.as_bytes(),
        log(1),
        view,
        &retries,
        0,
        limits(),
        WORKSPACE,
    )?;
    assert!(matches!(
        plan.decision(),
        RecordBatchApplyDecision::IntentConflict
    ));
    assert_eq!(
        outcomes.values().next().ok_or("outcome absent")?.response(),
        [0]
    );
    let mut expired = request(vec![create(b"i")?])?;
    expired.stamp = clock(101)?;
    let expired = encode_record_batch(expired, limits(), WORKSPACE)?;
    let plan = decide_record_batch(
        expired.as_bytes(),
        log(1),
        view,
        &retries,
        0,
        limits(),
        16_780_748
            + size_of::<Option<RecordBatchValueEffects>>()
            + std::mem::size_of::<wf_ports::log::DecodedRequest<'static>>(),
    )?;
    assert!(matches!(plan.decision(), RecordBatchApplyDecision::Expired));
    assert_operational_branches(view, original.as_bytes())?;
    Ok(())
}

fn assert_operational_branches(view: IndexedRequestView<'_>, original: &[u8]) -> TestResult {
    let retries: BTreeMap<&[u8], &InstalledRetryPolicy> = BTreeMap::new();
    let mut regenerated = request(vec![create(b"i")?])?;
    regenerated.stamp = clock(2)?;
    let regenerated = encode_record_batch(regenerated, limits(), WORKSPACE)?;
    assert!(matches!(
        decide_record_batch(
            regenerated.as_bytes(),
            log(0),
            view,
            &retries,
            100,
            limits(),
            WORKSPACE
        ),
        Err(RecordBatchApplyError::Apply(ApplyError::PositionConflict))
    ));
    let mut corrupt = original.to_vec();
    corrupt[29] ^= 1;
    assert!(matches!(
        decide_record_batch(&corrupt, log(1), view, &retries, 0, limits(), WORKSPACE),
        Err(RecordBatchApplyError::IntentMismatch)
    ));
    let foreign = wf_types::identity::GroupId::new(b"foreign".to_vec())?;
    let bad_group = IndexedRequestView {
        gate: view.gate,
        group: IndexedGroupView {
            batch: GroupView {
                group: &foreign,
                ..view.group.batch
            },
            ..view.group
        },
    };
    assert!(matches!(
        decide_record_batch(
            original,
            log(1),
            bad_group,
            &retries,
            0,
            limits(),
            WORKSPACE
        ),
        Err(RecordBatchApplyError::Apply(ApplyError::InvalidAuthority))
    ));
    Ok(())
}

fn running() -> TestResult<WorkflowRecord> {
    let created = reduce(None, &create(b"i")?.command)?
        .into_parts()
        .1
        .ok_or("create absent")?
        .record;
    let command = CommittedCommand {
        id: created.id().clone(),
        now: TimestampMillis::new(1),
        operation: Operation::Claim(Claim {
            expected_state: b"q".to_vec(),
            worker: b"w".to_vec(),
            token: LeaseToken::new(b"tok".to_vec())?,
            lease_ms: 100,
        }),
    };
    Ok(reduce(Some(&created), &command)?
        .into_parts()
        .1
        .ok_or("claim absent")?
        .record)
}

fn retry_request(record: &WorkflowRecord) -> TestResult<wf_ports::consensus::PreparedRequest> {
    let mut item = create(b"i")?;
    item.command.operation = Operation::RetryWithPolicy(PolicyRetry {
        guard: LeaseGuard {
            token: record.lease().ok_or("lease absent")?.token.clone(),
            fence: record.fence(),
        },
        run_at: None,
        policy_override: RetryOverride::default(),
        jitter: JitterSample::new(0),
        payload: Some(Vec::new()),
        error: Some(b"retry-error".to_vec()),
    });
    request(vec![item])
}

fn assert_current_retry_authority() -> TestResult {
    let current_record = running()?;
    let scope = scope()?;
    let entry = ClaimRecord {
        record: &current_record,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: 0,
    };
    let key = ScopedRecordId::for_record(&entry);
    let current = BTreeMap::from([(key, &current_record)]);
    let records = BTreeMap::from([(key, entry)]);
    let partition = create(b"i")?.partition;
    let placements = BTreeMap::from([(partition, scope.group().clone())]);
    let parallel = BTreeMap::from_iter(DueOrder::for_record(&entry).map(|due| (due, entry)));
    let fifo = BTreeMap::new();
    let policies = BTreeMap::new();
    let outcomes = BTreeMap::new();
    let view = IndexedRequestView {
        gate: GateView {
            scope: &scope,
            control: ControlState::initial(clock(0)?),
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
    let frame = encode_record_batch(retry_request(&current_record)?, limits(), WORKSPACE)?;
    for wrong_type in [false, true] {
        let policy = InstalledRetryPolicy::new(
            if wrong_type {
                b"other".to_vec()
            } else {
                b"r".to_vec()
            },
            RetryOverride {
                max_retries: Some(0),
                backoff: None,
                exhausted_to: Some(b"exhausted".to_vec()),
            },
            BTreeMap::new(),
        )?;
        let retries: BTreeMap<&[u8], &InstalledRetryPolicy> =
            BTreeMap::from([(b"r".as_slice(), &policy)]);
        let result = decide_record_batch(
            frame.as_bytes(),
            log(0),
            view,
            &retries,
            100,
            limits(),
            WORKSPACE,
        );
        if wrong_type {
            assert!(matches!(
                result,
                Err(RecordBatchApplyError::Apply(ApplyError::InvalidAuthority))
            ));
        } else {
            let plan: super::RecordBatchApplyPlan<'_> = result?;
            let RecordBatchApplyDecision::Fresh {
                operation: IndexedDecision::Accepted(batch),
                outcome,
            } = plan.decision()
            else {
                return Err("installed retry authority ignored".into());
            };
            let record = &batch.batch().items()[0]
                .plan()
                .write()
                .ok_or("retry write absent")?
                .record;
            assert_eq!(record.state(), b"exhausted");
            assert_eq!(record.attempts(), 1);
            assert_eq!(record.payload(), Some(b"".as_slice()));
            assert_eq!(record.error(), Some(b"retry-error".as_slice()));
            assert_eq!(outcome.response(), [0]);
        }
    }
    assert_eq!(current_record.state(), b"running");
    assert_eq!(current_record.version(), 2);
    Ok(())
}

#[test]
fn raw_batch_distinguishes_request_conflict_expiry_and_log_identity() -> TestResult {
    let (control, outcome) = original(false)?;
    assert_gate_branches(control, outcome)?;
    assert_current_retry_authority()?;
    let mut value = request(vec![create(b"i")?])?;
    value.stamp = clock(2)?;
    let encoded = encode_record_batch(value, limits(), WORKSPACE)?;
    with_empty(encoded.as_bytes(), limits(), WORKSPACE, |result| {
        assert!(matches!(
            result,
            Err(RecordBatchApplyError::Apply(ApplyError::InvalidAuthority))
        ));
        Ok(())
    })?;
    let frame = encode_record_batch(request(vec![create(b"i")?])?, limits(), WORKSPACE)?;
    let scope = scope()?;
    let placements = BTreeMap::from([(create(b"i")?.partition, scope.group().clone())]);
    let current = BTreeMap::new();
    let records = BTreeMap::new();
    let parallel = BTreeMap::new();
    let fifo = BTreeMap::new();
    let policies = BTreeMap::new();
    let retries: BTreeMap<&[u8], &InstalledRetryPolicy> = BTreeMap::new();
    let outcomes = BTreeMap::new();
    let view = IndexedRequestView {
        gate: GateView {
            scope: &scope,
            control: ControlState::initial(clock(0)?),
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
    assert!(matches!(
        decide_record_batch(
            frame.as_bytes(),
            log(0),
            view,
            &retries,
            0,
            limits(),
            WORKSPACE
        ),
        Err(RecordBatchApplyError::Retained(
            RequestValueError::InvalidExpiry
        ))
    ));
    Ok(())
}
