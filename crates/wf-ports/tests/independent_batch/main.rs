//! Prepared independent batches; mathematical plans establish no durable apply.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::mem::size_of;

use wf_domain::claim::{ClaimRecord, DueOrder, LaneKey, ScopedRecordId};
use wf_domain::lease::{ExtendLease, LeaseGuard, LeaseToken};
use wf_domain::policy::RetryOverride;
use wf_domain::retry::{JitterSample, PolicyRetry, Retry, RetryTime};
use wf_domain::workflow::admission::{AdmissionError, AdmissionLimits};
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::indexed::{
    IndependentIndexedDecision, IndependentIndexedPlan, IndexRejection, TouchedIndexedGroupView,
};
use wf_domain::workflow::request::gate::PointGateView;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedOutcome, RetainedStatus};
use wf_domain::workflow::{
    Cancel, CommittedCommand, Create, DomainError, Operation, Terminal, Transition, WorkflowRecord,
    reduce,
};
use wf_ports::blob::{RecordBatchValueEffect, RecordBatchValueEffects};
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use wf_ports::log::{
    CodecError, CodecLimits, FreshRecordBatch, RecordBatchApplyDecision, RecordBatchApplyError,
    RecordBatchApplyPlan, RecordBatchInspection, decide_fresh_independent_record_batch_touched,
    encode_independent_record_batch, inspect_independent_record_batch_point,
    prepare_independent_record_batch,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, Fence, HlcTimestamp, LogIdentity, TimestampMillis,
};

mod gate;
mod wire;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const WORKSPACE: usize = 64 * 1_024 * 1_024;

fn limits() -> CodecLimits {
    CodecLimits::new(AdmissionLimits::default())
}

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

fn log(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}

fn item(id: &[u8], operation: Operation) -> TestResult<PreparedBatchItem> {
    Ok(PreparedBatchItem {
        partition: PartitionId::new(b"p".to_vec())?,
        command: CommittedCommand {
            id: RecordId::new(id.to_vec())?,
            now: TimestampMillis::new(2),
            operation,
        },
        create_priority: 0,
    })
}

fn create() -> Operation {
    Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: None,
    })
}

fn guard() -> TestResult<LeaseGuard> {
    Ok(LeaseGuard {
        token: LeaseToken::new(b"k".to_vec())?,
        fence: Fence::new(1),
    })
}

fn terminal(fail: bool) -> TestResult<Operation> {
    let value = Terminal {
        guard: guard()?,
        payload: None,
        value: None,
    };
    Ok(if fail {
        Operation::Fail(value)
    } else {
        Operation::Complete(value)
    })
}

fn retry() -> TestResult<Operation> {
    Ok(Operation::Retry(Retry {
        guard: guard()?,
        run_at: RetryTime::Explicit(TimestampMillis::new(9)),
        payload: None,
        error: None,
    }))
}

fn cancel(fence: u64) -> Operation {
    Operation::Cancel(Cancel {
        fence: Fence::new(fence),
        lease_token: None,
        expected_state: None,
        expected_version: None,
        error: None,
    })
}

fn request(items: Vec<PreparedBatchItem>) -> TestResult<PreparedRequest> {
    Ok(PreparedRequest {
        scope: scope()?,
        request: RequestId::new(b"q".to_vec())?,
        intent: ClientIntent::new(1, [0xaa; 32])?,
        stamp: clock(2)?,
        operation: PreparedOperation::Batch(items),
    })
}

fn claimed(id: &[u8]) -> TestResult<WorkflowRecord> {
    let mut command = item(id, create())?.command;
    command.now = TimestampMillis::new(1);
    let queued = reduce(None, &command)?
        .write()
        .ok_or("create write")?
        .record
        .clone();
    command.operation = Operation::Claim(wf_domain::claim::Claim {
        expected_state: b"q".to_vec(),
        worker: b"w".to_vec(),
        token: LeaseToken::new(b"k".to_vec())?,
        lease_ms: 1_000,
    });
    Ok(reduce(Some(&queued), &command)?
        .write()
        .ok_or("claim write")?
        .record
        .clone())
}

fn with_view<'current, T>(
    records: &'current [WorkflowRecord],
    ids: &[&[u8]],
    inspect: impl FnOnce(TouchedIndexedGroupView<'_, 'current>) -> TestResult<T>,
) -> TestResult<T> {
    let group = scope()?.group().clone();
    let placements = BTreeMap::from([(PartitionId::new(b"p".to_vec())?, group.clone())]);
    let rows: Vec<_> = records
        .iter()
        .map(|record| ClaimRecord {
            record,
            partition_key: b"p",
            priority: 0,
            state_enter_seq: 99,
        })
        .collect();
    let current = ids
        .iter()
        .map(|id| {
            (
                ScopedRecordId {
                    partition_key: b"p",
                    id,
                },
                rows.iter()
                    .copied()
                    .find(|row| row.record.id().as_bytes() == *id),
            )
        })
        .collect();
    let parallel = rows
        .iter()
        .filter_map(|row| DueOrder::for_record(row).map(|due| (due, true)))
        .collect();
    let members = BTreeMap::new();
    let heads = rows
        .iter()
        .filter_map(|row| LaneKey::for_record(row).map(|lane| (lane, None)))
        .collect();
    let policies = BTreeMap::from([(b"r".as_slice(), None)]);
    let retries = BTreeMap::from([(b"r".as_slice(), None)]);
    inspect(TouchedIndexedGroupView {
        group: &group,
        placements: &placements,
        current: &current,
        parallel: &parallel,
        fifo_members: &members,
        fifo_heads: &heads,
        policies: &policies,
        retry_policies: &retries,
    })
}

fn fresh(encoded: &[u8], codec: CodecLimits, workspace: usize) -> TestResult<FreshRecordBatch<'_>> {
    let scope = scope()?;
    let decoded = prepare_independent_record_batch(encoded, codec, workspace)?;
    let inspection = inspect_independent_record_batch_point(
        decoded,
        log(0),
        PointGateView {
            scope: &scope,
            control: ControlState::initial(clock(0)?),
            outcome: None,
            retained_nonempty: false,
        },
        codec,
        workspace,
    )?;
    let RecordBatchInspection::Fresh(fresh) = inspection else {
        return Err("not fresh".into());
    };
    Ok(fresh)
}

fn decide<'current>(
    records: &'current [WorkflowRecord],
    ids: &[&[u8]],
    encoded: &[u8],
    codec: CodecLimits,
    workspace: usize,
    live: usize,
) -> TestResult<RecordBatchApplyPlan<'current>> {
    let fresh = fresh(encoded, codec, workspace)?;
    with_view(records, ids, |view| {
        Ok(decide_fresh_independent_record_batch_touched(
            fresh, view, live, 1_000,
        )?)
    })
}

fn accepted<'plan, 'current>(
    plan: &'plan RecordBatchApplyPlan<'current>,
) -> TestResult<&'plan IndependentIndexedPlan<'current>> {
    let RecordBatchApplyDecision::IndependentFresh {
        operation: IndependentIndexedDecision::Accepted(value),
        ..
    } = plan.decision()
    else {
        return Err("not independent accepted".into());
    };
    Ok(value)
}

fn outcome<'plan>(plan: &'plan RecordBatchApplyPlan<'_>) -> TestResult<&'plan RetainedOutcome> {
    let RecordBatchApplyDecision::IndependentFresh { outcome, .. } = plan.decision() else {
        return Err("not independent outcome".into());
    };
    Ok(outcome)
}

fn hex(value: &str) -> TestResult<Vec<u8>> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn eight_operations() -> TestResult<Vec<PreparedBatchItem>> {
    let mut stale = terminal(true)?;
    if let Operation::Fail(value) = &mut stale {
        value.guard.token = LeaseToken::new(b"bad".to_vec())?;
    }
    Ok(vec![
        item(b"c", create())?,
        item(
            b"c",
            Operation::Transition(Transition {
                from: b"q".to_vec(),
                to: b"z".to_vec(),
                fence: Fence::new(0),
                lease_token: None,
                run_at: None,
                payload: None,
            }),
        )?,
        item(b"c", cancel(0))?,
        item(
            b"i",
            Operation::ExtendLease(ExtendLease {
                guard: guard()?,
                lease_ms: 2_000,
            }),
        )?,
        item(b"i", stale)?,
        item(b"i", terminal(true)?)?,
        item(
            b"j",
            Operation::RetryWithPolicy(PolicyRetry {
                guard: guard()?,
                run_at: None,
                policy_override: RetryOverride::default(),
                jitter: JitterSample::new(0),
                payload: None,
                error: None,
            }),
        )?,
        item(b"j", cancel(1))?,
        item(b"k", terminal(false)?)?,
        item(b"k", terminal(false)?)?,
        item(b"l", retry()?)?,
        item(b"l", cancel(1))?,
    ])
}

#[test]
fn one_gate_all_histories_fresh() -> TestResult {
    let records = [
        claimed(b"i")?,
        claimed(b"j")?,
        claimed(b"k")?,
        claimed(b"l")?,
    ];
    let plan = {
        let prepared = request(eight_operations()?)?;
        let bytes = encode_independent_record_batch(prepared, limits(), WORKSPACE)?;
        decide(
            &records,
            &[b"c", b"i", b"j", b"k", b"l"],
            bytes.as_bytes(),
            limits(),
            WORKSPACE,
            0,
        )?
    };
    let batch = accepted(&plan)?;
    assert_eq!(batch.results().len(), 12);
    assert_eq!(
        batch.results()[4],
        Err(IndexRejection::Batch(BatchRejection::Domain(
            DomainError::StaleLease
        )))
    );
    assert!(
        batch.results()[9]
            .as_ref()
            .map_err(|_| "noop result")?
            .plan()
            .write()
            .is_none()
    );
    assert_eq!(batch.final_indices().collect::<Vec<_>>(), [2, 5, 7, 8, 11]);
    assert_eq!(
        batch
            .originals()
            .map(|entry| entry.map(|row| row.record.version()))
            .collect::<Vec<_>>(),
        [None, Some(2), Some(2), Some(2), Some(2)]
    );
    assert_eq!(batch.history_plans().count(), 10);
    assert_eq!(batch.consumed_ordinals(), 2);
    for (index, version) in [(2, 3), (5, 4), (7, 4), (8, 3), (11, 4)] {
        let write = batch.results()[index]
            .as_ref()
            .map_err(|_| "final result")?
            .plan()
            .write()
            .ok_or("final write")?;
        assert_eq!(write.record.version(), version);
    }
    let expected = [vec![2, 0, 0, 0, 12], vec![0; 4], vec![1, 0, 4], vec![0; 7]].concat();
    assert_eq!(outcome(&plan)?.response(), expected);
    assert_eq!(outcome(&plan)?.intent().version(), 6);
    assert_eq!(outcome(&plan)?.original_position(), log(0));
    assert_eq!(outcome(&plan)?.original_time(), TimestampMillis::new(2));
    assert_eq!(outcome(&plan)?.status(), RetainedStatus::Accepted);
    assert_eq!(
        plan.control()
            .next()
            .last_entry()
            .map(wf_domain::workflow::control::EntryIdentity::log),
        Some(log(0))
    );
    assert_index_refusal()?;
    Ok(())
}

fn assert_index_refusal() -> TestResult {
    let mut bad = item(b"c", create())?;
    bad.create_priority = 3;
    let bytes = encode_independent_record_batch(
        request(vec![bad, item(b"c", create())?, item(b"c", cancel(0))?])?,
        limits(),
        WORKSPACE,
    )?;
    let plan = decide(&[], &[b"c"], bytes.as_bytes(), limits(), WORKSPACE, 0)?;
    let batch = accepted(&plan)?;
    assert_eq!(batch.results()[0], Err(IndexRejection::InvalidPriority));
    assert_eq!(batch.final_indices().collect::<Vec<_>>(), [2]);
    assert_eq!(batch.history_plans().count(), 2);
    assert_eq!(batch.consumed_ordinals(), 1);
    assert_eq!(outcome(&plan)?.response(), [2, 0, 0, 0, 3, 1, 4, 0, 0, 0]);
    Ok(())
}

#[test]
fn prospective_workspace_and_profiles() -> TestResult {
    let decoded_header = std::mem::size_of::<wf_ports::log::DecodedRequest<'static>>();
    let decoded_bound = 16_780_748 + decoded_header;
    let input_bound = 33_558_419 + decoded_header;
    let encoded = encode_independent_record_batch(
        request(vec![item(b"c", create())?])?,
        limits(),
        WORKSPACE,
    )?;
    assert_eq!(encoded.as_bytes().len(), 115);
    {
        let (maximum, attempted) = (16_780_747, 16_780_748);
        assert!(
            matches!(prepare_independent_record_batch(encoded.as_bytes(), limits(), maximum),
            Err(RecordBatchApplyError::Workspace { attempted: actual, maximum: cap }) if actual == attempted && cap == maximum)
        );
    }
    assert!(matches!(
        prepare_independent_record_batch(encoded.as_bytes(), limits(), decoded_bound - 1),
        Err(RecordBatchApplyError::Workspace { attempted, maximum })
            if attempted == decoded_bound && maximum == attempted - 1
    ));
    assert!(prepare_independent_record_batch(encoded.as_bytes(), limits(), decoded_bound).is_ok());
    let owner_bytes =
        size_of::<Option<RecordBatchValueEffects>>() + size_of::<RecordBatchValueEffect>();
    let owned = input_bound + owner_bytes;
    for (maximum, live, attempted) in [
        (33_558_411, 0, 33_558_412),
        (33_558_418, 7, 33_558_419),
        (input_bound - 1, 7, input_bound),
        (owned - 1, 7, owned),
    ] {
        let f = fresh(encoded.as_bytes(), limits(), maximum)?;
        with_view(&[], &[b"c"], |view| {
            assert!(
                matches!(decide_fresh_independent_record_batch_touched(f, view, live, 1_000),
                Err(RecordBatchApplyError::Workspace { attempted: actual, maximum: cap }) if actual == attempted && cap == maximum)
            );
            Ok(())
        })?;
    }
    assert_eq!(
        accepted(&decide(
            &[],
            &[b"c"],
            encoded.as_bytes(),
            limits(),
            owned,
            7
        )?)?
        .results()
        .len(),
        1
    );
    for (cap, rejected) in [(2_868, false), (2_867, true)] {
        let codec = CodecLimits::new(AdmissionLimits::new(256, cap, 1)?);
        let bytes = encode_independent_record_batch(
            request(vec![item(b"c", create())?])?,
            codec,
            WORKSPACE,
        )?;
        let plan = decide(&[], &[b"c"], bytes.as_bytes(), codec, WORKSPACE, 0)?;
        if rejected {
            assert_eq!(outcome(&plan)?.response(), [1, 1, 4]);
            assert!(matches!(
                plan.decision(),
                RecordBatchApplyDecision::IndependentFresh {
                    operation: IndependentIndexedDecision::Rejected(IndexRejection::Batch(
                        BatchRejection::Admission(AdmissionError::DecisionBytes)
                    )),
                    ..
                }
            ));
        } else {
            assert_eq!(accepted(&plan)?.history_plans().count(), 1);
        }
    }
    let empty = request(Vec::new())?;
    assert!(matches!(
        encode_independent_record_batch(empty, limits(), WORKSPACE),
        Err(RecordBatchApplyError::Encoding(CodecError::ItemLimit))
    ));
    Ok(())
}
