//! Mathematical touched facts and real encoding/gate/producer setup only.

use std::collections::BTreeMap;

use wf_domain::claim::{Claim, ClaimRecord, DueOrder, LaneKey, ScopedRecordId};
use wf_domain::lease::LeaseToken;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::caller::{MutationFieldValue, MutationInput, normalize_core_mutation};
use wf_domain::workflow::control::{
    ControlState, EntryIdentity, EntryStamp, PreparedDigest, plan_control,
};
use wf_domain::workflow::indexed::TouchedIndexedGroupView;
use wf_domain::workflow::request::gate::PointGateView;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedOutcome};
use wf_domain::workflow::{
    CommittedCommand, Create, Operation, StartAndClaim, WorkflowRecord, reduce,
};
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use wf_ports::log::{
    CodecLimits, DecodedRequest, PreparedBytes, RecordBatchApplyDecision, RecordBatchApplyError,
    RecordBatchApplyPlan, RecordBatchInspection, decide_fresh_atomic_start_touched,
    decide_fresh_independent_record_batch_touched, decide_fresh_public_mutation_touched,
    decide_fresh_record_batch_touched, encode_atomic_start, encode_independent_record_batch,
    encode_public_mutation, encode_record_batch, inspect_atomic_start_point,
    inspect_independent_record_batch_point, inspect_public_mutation_point,
    inspect_record_batch_point, prepare_atomic_start, prepare_independent_record_batch,
    prepare_public_mutation, prepare_record_batch,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity, TimestampMillis,
};

pub(super) type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

pub(super) const WORKSPACE: usize = 64 * 1_024 * 1_024;

pub(super) fn limits() -> CodecLimits {
    CodecLimits::new(AdmissionLimits::default())
}

pub(super) fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    ))
}

pub(super) fn clock(now: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?)
}

pub(super) fn log(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}

pub(super) fn control() -> TestResult<ControlState> {
    Ok(plan_control(
        ControlState::initial(clock(0)?),
        EntryIdentity::new(log(0), PreparedDigest::new([0x31; 32])),
        EntryStamp::Unstamped,
    )?
    .next())
}

pub(super) fn item(id: &[u8], operation: Operation) -> TestResult<PreparedBatchItem> {
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

pub(super) fn request(items: Vec<PreparedBatchItem>) -> TestResult<PreparedRequest> {
    Ok(PreparedRequest {
        scope: scope()?,
        request: RequestId::new(b"q".to_vec())?,
        intent: ClientIntent::new(1, [0xaa; 32])?,
        stamp: clock(2)?,
        operation: PreparedOperation::Batch(items),
    })
}

pub(super) fn batch_frame(
    items: Vec<PreparedBatchItem>,
    independent: bool,
) -> TestResult<PreparedBytes> {
    let input = request(items)?;
    Ok(if independent {
        encode_independent_record_batch(input, limits(), WORKSPACE)?
    } else {
        encode_record_batch(input, limits(), WORKSPACE)?
    })
}

pub(super) fn public_frame<'a>(
    operation: &'a str,
    id: &'a [u8],
    fields: &'a [(&'a str, MutationFieldValue<'a>)],
) -> TestResult<PreparedBytes> {
    let normalized = normalize_core_mutation(
        MutationInput {
            operation,
            id,
            fields,
        },
        &[],
        Some(b"p"),
        AdmissionLimits::default(),
        WORKSPACE,
    )?;
    let (encoded, _) = encode_public_mutation(
        &scope()?,
        &RequestId::new(b"q".to_vec())?,
        &normalized,
        clock(2)?,
        limits(),
        WORKSPACE,
    )?;
    Ok(encoded)
}

pub(super) fn atomic_frame(payload: Option<&[u8]>) -> TestResult<PreparedBytes> {
    let input = item(
        b"i",
        Operation::StartAndClaim(StartAndClaim {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            worker: b"w".to_vec(),
            lease_ms: 1_000,
            payload: payload.map(<[u8]>::to_vec),
        }),
    )?;
    Ok(encode_atomic_start(
        request(vec![input])?,
        limits(),
        WORKSPACE,
    )?)
}

pub(super) fn claimed(id: &[u8], payload: Option<&[u8]>) -> TestResult<WorkflowRecord> {
    let created = reduce(
        None,
        &CommittedCommand {
            id: RecordId::new(id.to_vec())?,
            now: TimestampMillis::new(1),
            operation: Operation::Create(Create {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                run_at: None,
                payload: payload.map(<[u8]>::to_vec),
            }),
        },
    )?;
    let (_, write, _) = created.into_parts();
    let current = write.ok_or("create fixture lacks a write")?.record;
    let next = reduce(
        Some(&current),
        &CommittedCommand {
            id: RecordId::new(id.to_vec())?,
            now: TimestampMillis::new(1),
            operation: Operation::Claim(Claim {
                expected_state: b"q".to_vec(),
                worker: b"w".to_vec(),
                token: LeaseToken::new(b"k".to_vec())?,
                lease_ms: 1_000,
            }),
        },
    )?;
    let (_, write, _) = next.into_parts();
    Ok(write.ok_or("claim fixture lacks a write")?.record)
}

pub(super) fn with_view<'current, T>(
    records: &'current [WorkflowRecord],
    ids: &[&[u8]],
    check: impl FnOnce(TouchedIndexedGroupView<'_, 'current>) -> TestResult<T>,
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
    let mut heads: BTreeMap<_, _> = rows
        .iter()
        .filter_map(|row| LaneKey::for_record(row).map(|lane| (lane, None)))
        .collect();
    heads.insert(
        LaneKey {
            workflow_type: b"r",
            state: b"q",
            partition_key: b"p",
        },
        None,
    );
    let policies = BTreeMap::from([(b"r".as_slice(), None)]);
    let retries = BTreeMap::from([(b"r".as_slice(), None)]);
    check(TouchedIndexedGroupView {
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

pub(super) fn prepare(
    profile: u16,
    bytes: &[u8],
    workspace: usize,
) -> TestResult<Result<DecodedRequest<'_>, RecordBatchApplyError>> {
    Ok(match profile {
        1 => prepare_public_mutation(bytes, limits(), workspace),
        5 => prepare_record_batch(bytes, limits(), workspace),
        6 => prepare_independent_record_batch(bytes, limits(), workspace),
        7 => prepare_atomic_start(bytes, limits(), workspace),
        _ => return Err("unsupported fixture profile".into()),
    })
}

pub(super) fn inspection<'input, 'retained>(
    profile: u16,
    bytes: &'input [u8],
    workspace: usize,
    control: ControlState,
    original: Option<&'retained RetainedOutcome>,
    index: u64,
) -> TestResult<Result<RecordBatchInspection<'input, 'retained>, RecordBatchApplyError>> {
    let decoded = match prepare(profile, bytes, workspace)? {
        Ok(decoded) => decoded,
        Err(error) => return Ok(Err(error)),
    };
    let query = scope()?;
    let view = PointGateView {
        scope: &query,
        control,
        retained_nonempty: original.is_some(),
        outcome: original,
    };
    Ok(match profile {
        1 => inspect_public_mutation_point(decoded, log(index), view, limits(), workspace),
        5 => inspect_record_batch_point(decoded, log(index), view, limits(), workspace),
        6 => inspect_independent_record_batch_point(decoded, log(index), view, limits(), workspace),
        7 => inspect_atomic_start_point(decoded, log(index), view, limits(), workspace),
        _ => return Err("unsupported fixture profile".into()),
    })
}

pub(super) fn decide<'current>(
    profile: u16,
    records: &'current [WorkflowRecord],
    ids: &[&[u8]],
    bytes: &[u8],
    workspace: usize,
    additional_live_bytes: usize,
) -> TestResult<Result<RecordBatchApplyPlan<'current>, RecordBatchApplyError>> {
    let inspected = match inspection(profile, bytes, workspace, control()?, None, 1)? {
        Ok(inspected) => inspected,
        Err(error) => return Ok(Err(error)),
    };
    let fresh = match inspected {
        RecordBatchInspection::Fresh(fresh) => fresh,
        RecordBatchInspection::Complete(_) => {
            return Err("fixture expected a real Fresh gate".into());
        }
    };
    with_view(records, ids, |view| {
        Ok(match profile {
            1 => decide_fresh_public_mutation_touched(fresh, view, additional_live_bytes, 1_000),
            5 => decide_fresh_record_batch_touched(fresh, view, additional_live_bytes, 1_000),
            6 => decide_fresh_independent_record_batch_touched(
                fresh,
                view,
                additional_live_bytes,
                1_000,
            ),
            7 => decide_fresh_atomic_start_touched(fresh, view, additional_live_bytes, 1_000),
            _ => return Err("unsupported fixture profile".into()),
        })
    })
}

pub(super) fn outcome<'plan>(
    plan: &'plan RecordBatchApplyPlan<'_>,
) -> TestResult<&'plan RetainedOutcome> {
    match plan.decision() {
        RecordBatchApplyDecision::Fresh { outcome, .. }
        | RecordBatchApplyDecision::IndependentFresh { outcome, .. } => Ok(outcome),
        _ => Err("fixture expected a real Fresh retained outcome".into()),
    }
}
