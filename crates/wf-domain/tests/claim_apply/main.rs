//! Pure apply-selected claim controls; no store or durable adapter is simulated.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::claim::apply::{
    ClaimApplyView, ClaimDecision, ClaimEncoding, ClaimEnvelope, ClaimQuery, ClaimRejection,
    ClaimStates, ClaimWorkLimits,
};
use wf_domain::claim::{
    Claim, ClaimRecord, DueOrder, LaneKey, LaneOrder, PartitionFilter, ScopedRecordId,
    SelectionStop,
};
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::policy::definition::{
    FieldPatch, PolicyPatch, PolicySnapshot, StateChange, StateMode, StatePolicyPatch,
};
use wf_domain::policy::installation::{
    InstallCommand, PolicyCandidate, PolicyLimits, prepare_install,
};
use wf_domain::workflow::admission::{AdmissionError, AdmissionLimits};
use wf_domain::workflow::batch::{ApplyError, GroupView};
use wf_domain::workflow::control::{
    ControlState, EntryIdentity, EntryStamp, PreparedDigest, plan_control,
};
use wf_domain::workflow::indexed::IndexedGroupView;
use wf_domain::workflow::request::gate::{CommittedHeader, GateView};
use wf_domain::workflow::request::{
    ClientIntent, DispatchDecision, DispatchPlan, RequestScope, RetainedMetadata, RetainedOutcome,
    RetainedStatus, decide_claim_request,
};
use wf_domain::workflow::{
    CommittedCommand, Create, Operation, Outcome, Terminal, WorkflowRecord, reduce,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity, TimestampMillis,
};

mod activated_priority;
mod admission;
mod fifo_indexes;
mod fifo_middle_cancel;
mod fifo_transition_unblock;
mod observed;
mod replay_models;
mod source_scenarios;

type TestError = Box<dyn std::error::Error>;
type TestResult = Result<(), TestError>;

fn test_scope() -> Result<RequestScope, TestError> {
    Ok(RequestScope::new(
        TenantId::new(b"tenant".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"recovery".to_vec())?,
    ))
}
fn test_identity(index: u64) -> EntryIdentity {
    EntryIdentity::new(
        LogIdentity::new(1, 7, AppliedPosition::new(index)),
        PreparedDigest::new([17; 32]),
    )
}
fn test_stamp(now: u64, wall: u64) -> Result<CommittedTime, TestError> {
    Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, wall)?)
}
fn test_control() -> Result<ControlState, TestError> {
    Ok(plan_control(
        ControlState::initial(test_stamp(0, 0)?),
        test_identity(0),
        EntryStamp::Unstamped,
    )?
    .next())
}
fn test_header<'a>(
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
        identity: test_identity(index),
        stamp: test_stamp(now, now)?,
        encoded_entry: b"claim-entry",
        encoded_isolation_scope: b"bound-scope",
    })
}
fn test_record(
    id: &[u8],
    kind: &[u8],
    state: &[u8],
    payload: Option<Vec<u8>>,
    created: u64,
    due: u64,
) -> Result<WorkflowRecord, TestError> {
    let command = CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(created),
        operation: Operation::Create(Create {
            workflow_type: kind.to_vec(),
            state: state.to_vec(),
            run_at: Some(TimestampMillis::new(due)),
            payload,
        }),
    };
    Ok(reduce(None, &command)?
        .into_parts()
        .1
        .ok_or("missing fixture record")?
        .record)
}
fn test_claimed(
    record: &WorkflowRecord,
    worker: &[u8],
    now: u64,
    lease: u64,
) -> Result<WorkflowRecord, TestError> {
    let command = CommittedCommand {
        id: record.id().clone(),
        now: TimestampMillis::new(now),
        operation: Operation::Claim(Claim {
            expected_state: record.state().to_vec(),
            worker: worker.to_vec(),
            token: LeaseToken::new(b"fixture-token".to_vec())?,
            lease_ms: lease,
        }),
    };
    Ok(reduce(Some(record), &command)?
        .into_parts()
        .1
        .ok_or("missing fixture claim")?
        .record)
}
fn test_fifo_policy(states: &[&[u8]]) -> Result<PolicyCandidate, TestError> {
    let patch = PolicyPatch {
        states: states
            .iter()
            .map(|state| {
                (
                    state.to_vec(),
                    StateChange::Patch(StatePolicyPatch {
                        mode: FieldPatch::Set(StateMode::Fifo),
                        ..StatePolicyPatch::default()
                    }),
                )
            })
            .collect(),
        ..PolicyPatch::default()
    };
    Ok(prepare_install(
        None,
        0,
        &InstallCommand::low_level(b"job".to_vec(), patch),
        PolicyLimits::default(),
    )?)
}
fn test_query(kind: &[u8], limit: usize) -> ClaimQuery<'_> {
    ClaimQuery {
        workflow_type: kind,
        partitions: PartitionFilter::Any,
        states: ClaimStates::Any,
        exclude_states: &[],
        priority: None,
        worker: b"worker-a",
        lease_ms: 50,
        limit,
    }
}
fn test_envelope<'a>(header: CommittedHeader<'a>, query: ClaimQuery<'a>) -> ClaimEnvelope<'a> {
    ClaimEnvelope {
        header,
        query,
        token_version: 1,
        first_ordinal: u32::MAX,
        scan_budget: 16_384,
        head_budget: 4_096,
        resume: None,
    }
}
fn with_claim_view<T>(
    entries: &[ClaimRecord<'_>],
    partitions: &[PartitionId],
    gate: GateView<'_>,
    policies: &BTreeMap<&[u8], &PolicySnapshot>,
    revision: u64,
    run: impl for<'view> FnOnce(ClaimApplyView<'view>) -> Result<T, TestError>,
) -> Result<T, TestError> {
    let placements = partitions
        .iter()
        .map(|p| (p.clone(), gate.scope.group().clone()))
        .collect();
    let records = entries
        .iter()
        .map(|e| (ScopedRecordId::for_record(e), *e))
        .collect();
    let canonical = entries
        .iter()
        .map(|e| (ScopedRecordId::for_record(e), e.record))
        .collect();
    let mut parallel = BTreeMap::new();
    let mut fifo = BTreeMap::new();
    let mut keys = Vec::new();
    for entry in entries {
        let mode = policies
            .get(entry.record.workflow_type())
            .and_then(|policy| policy.states().get(entry.logical_state()))
            .and_then(|state| state.mode);
        if mode == Some(StateMode::Fifo) {
            if let Some(lane) = LaneKey::for_record(entry) {
                fifo.entry(lane)
                    .or_insert_with(BTreeMap::new)
                    .insert(LaneOrder::for_record(entry), *entry);
            }
        } else if let Some(order) = DueOrder::for_record(entry) {
            parallel.insert(order, *entry);
        }
        let mut key = b"f:{".to_vec();
        key.extend_from_slice(entry.partition_key);
        key.extend_from_slice(b"}:s:");
        key.extend_from_slice(entry.record.id().as_bytes());
        keys.push(key);
    }
    let mut encodings = BTreeMap::new();
    for (entry, key) in entries.iter().zip(&keys) {
        let partition = partitions
            .iter()
            .find(|p| p.as_bytes() == entry.partition_key)
            .ok_or("missing fixture partition")?;
        encodings.insert(
            ScopedRecordId::for_record(entry),
            ClaimEncoding {
                partition,
                state_key: key,
            },
        );
    }
    run(ClaimApplyView {
        gate,
        group: IndexedGroupView {
            batch: GroupView {
                group: gate.scope.group(),
                placements: &placements,
                records: &canonical,
            },
            records: &records,
            parallel: &parallel,
            fifo: &fifo,
            policies,
        },
        revision,
        encodings: &encodings,
    })
}

#[test]
fn source_future_due_boundary_preserves_empty_and_singleton_claims() -> TestResult {
    // Pinned invariant_test.exs:1291–1321, declaration 4bda857b47effb76.
    let record = test_record(
        b"flow-invariant-future-due",
        b"flow-invariant-type",
        b"queued",
        Some(b"payload".to_vec()),
        1_000,
        10_000,
    )?;
    assert_eq!(
        (
            record.state(),
            record.payload(),
            record.created_at(),
            record.next_run_at()
        ),
        (
            b"queued".as_slice(),
            Some(b"payload".as_slice()),
            TimestampMillis::new(1_000),
            Some(TimestampMillis::new(10_000))
        )
    );
    let scope = test_scope()?;
    let requests = [
        RequestId::new(b"future-due-before".to_vec())?,
        RequestId::new(b"future-due-at".to_vec())?,
    ];
    let entries = [ClaimRecord {
        record: &record,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: 0,
    }];
    let partitions = [PartitionId::new(b"p".to_vec())?];
    let outcomes = BTreeMap::new();
    let states = [b"queued".as_slice()];
    let query = ClaimQuery {
        states: ClaimStates::Only(&states),
        worker: b"invariant-worker",
        lease_ms: 30_000,
        ..test_query(b"flow-invariant-type", 10)
    };
    with_claim_view(
        &entries,
        &partitions,
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::new(),
        1,
        |view| {
            let mut control = view.gate.control;
            for (index, now, request) in [(1, 9_999, &requests[0]), (2, 10_000, &requests[1])] {
                let current = ClaimApplyView {
                    gate: GateView {
                        control,
                        ..view.gate
                    },
                    ..view
                };
                let plan = decide_claim_request(
                    current,
                    &test_envelope(test_header(&scope, request, index, now)?, query),
                    AdmissionLimits::default(),
                    ClaimWorkLimits::default(),
                )?;
                let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision()
                else {
                    return Err("source future-due call was not fresh acceptance".into());
                };
                if index == 1 {
                    assert!(claim.batch().is_none());
                } else {
                    let batch = claim.batch().ok_or("source due claim has no batch")?;
                    assert_eq!(batch.batch().items().len(), 1);
                    let write = batch.batch().items()[0]
                        .plan()
                        .write()
                        .ok_or("source due claim has no record write")?;
                    assert_eq!(write.record.id(), record.id());
                }
                control = plan.control().next();
            }
            Ok(())
        },
    )
}

#[test]
fn source_leased_and_terminal_records_are_absent_from_later_claims() -> TestResult {
    // Pinned invariant_test.exs:1246–1289, declaration 0bf563deac76e307.
    let id = b"flow-invariant-leased-terminal";
    let kind = b"flow-invariant-type";
    let mut record = test_record(id, kind, b"queued", Some(b"payload".to_vec()), 1_000, 1_000)?;
    let created = (b"queued".as_slice(), Some(b"payload".as_slice()));
    assert_eq!((record.state(), record.payload()), created);
    let scope = test_scope()?;
    let partitions = [PartitionId::new(b"p".to_vec())?];
    let outcomes = BTreeMap::new();
    let states = [b"queued".as_slice()];
    let mut control = test_control()?;
    for (index, now) in [(1_u64, 1_000), (2, 1_001), (3, 1_100), (4, 2_000)] {
        if index == 3 {
            let stamp = EntryStamp::Stamped(test_stamp(now, now)?);
            let completion_control = plan_control(control, test_identity(index), stamp)?;
            let lease = record.lease().ok_or("source claim has no current lease")?;
            let guard = LeaseGuard {
                token: lease.token.clone(),
                fence: record.fence(),
            };
            let command = CommittedCommand {
                id: record.id().clone(),
                now: completion_control.execution_time(),
                operation: Operation::Complete(Terminal {
                    guard,
                    payload: None,
                    value: Some(b"done".to_vec()),
                }),
            };
            let (outcome, write, _) = reduce(Some(&record), &command)?.into_parts();
            assert_eq!(outcome, Outcome::Changed);
            record = write.ok_or("source completion has no write")?.record;
            let completed = (b"completed".as_slice(), Some(b"done".as_slice()));
            assert_eq!((record.state(), record.result()), completed);
            control = completion_control.next();
            continue;
        }
        let request = RequestId::new(index.to_be_bytes().to_vec())?;
        let header = test_header(&scope, &request, index, now)?;
        let entries = [ClaimRecord {
            record: &record,
            partition_key: b"p",
            priority: 0,
            state_enter_seq: 0,
        }];
        let limit = if index == 1 { 1 } else { 10 };
        let query = ClaimQuery {
            states: ClaimStates::Only(&states),
            worker: b"invariant-worker",
            lease_ms: 30_000,
            ..test_query(kind, limit)
        };
        let (next_control, next_record) = with_claim_view(
            &entries,
            &partitions,
            GateView {
                scope: &scope,
                control,
                outcomes: &outcomes,
            },
            &BTreeMap::new(),
            record.version(),
            |view| {
                let envelope = test_envelope(header, query);
                let plan = decide_claim_request(
                    view,
                    &envelope,
                    AdmissionLimits::default(),
                    ClaimWorkLimits::default(),
                )?;
                let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision()
                else {
                    return Err("source leased/terminal claim was not fresh acceptance".into());
                };
                let next_record = if index == 1 {
                    let batch = claim.batch().ok_or("source first claim has no batch")?;
                    let item = batch.batch().items().first().ok_or("empty source claim")?;
                    let item_plan = item.plan();
                    assert_eq!(item_plan.outcome(), Outcome::Changed);
                    let write = item_plan.write().ok_or("missing source claim write")?;
                    assert_eq!(write.record.id(), record.id());
                    assert_eq!(batch.batch().items().len(), 1);
                    Some(write.record.clone())
                } else {
                    let empty = (true, SelectionStop::Complete);
                    assert_eq!((claim.batch().is_none(), claim.stop()), empty);
                    None
                };
                Ok((plan.control().next(), next_record))
            },
        )?;
        control = next_control;
        record = next_record.unwrap_or(record);
    }
    Ok(())
}

#[test]
fn source_fifo_activation_preserves_populated_state_entry_order() -> TestResult {
    // Pinned invariant_test.exs:772–800, declaration 587581adc9eb0ceb.
    let kind = b"flow-invariant-fifo-activation-type".as_slice();
    let partition = b"tenant:fifo-activation".as_slice();
    let ids = [
        b"z-fifo-activation-first".as_slice(),
        b"a-fifo-activation-second".as_slice(),
    ];
    let mut rows = [
        test_record(ids[0], kind, b"queued", Some(ids[0].to_vec()), 1_000, 2_000)?,
        test_record(ids[1], kind, b"queued", Some(ids[1].to_vec()), 1_001, 2_000)?,
    ];
    let patch = PolicyPatch {
        states: BTreeMap::from([(
            b"queued".to_vec(),
            StateChange::Patch(StatePolicyPatch {
                mode: FieldPatch::Set(StateMode::Fifo),
                ..StatePolicyPatch::default()
            }),
        )]),
        ..PolicyPatch::default()
    };
    let command = InstallCommand::low_level(kind.to_vec(), patch);
    let policy = prepare_install(None, 0, &command, PolicyLimits::default())?;
    let policies = BTreeMap::from([(kind, policy.policy())]);
    let scope = test_scope()?;
    let partitions = [PartitionId::new(partition.to_vec())?];
    let outcomes = BTreeMap::new();
    let filter = [partition];
    let states = [b"queued".as_slice()];
    let query = ClaimQuery {
        partitions: PartitionFilter::Only(&filter),
        states: ClaimStates::Only(&states),
        worker: b"fifo-worker",
        lease_ms: 30_000,
        ..test_query(kind, 10)
    };
    let mut control = test_control()?;
    for (index, now) in [(1_u64, 2_000), (2, 2_001)] {
        let request = RequestId::new(index.to_be_bytes().to_vec())?;
        let entries = rows
            .iter()
            .enumerate()
            .map(|(sequence, record)| ClaimRecord {
                record,
                partition_key: partition,
                priority: 0,
                state_enter_seq: 1 + sequence as u128,
            })
            .collect::<Vec<_>>();
        let (next_control, write) = with_claim_view(
            &entries,
            &partitions,
            GateView {
                scope: &scope,
                control,
                outcomes: &outcomes,
            },
            &policies,
            index,
            |view| {
                let header = test_header(&scope, &request, index, now)?;
                let plan = decide_claim_request(
                    view,
                    &test_envelope(header, query),
                    AdmissionLimits::default(),
                    ClaimWorkLimits::default(),
                )?;
                let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision()
                else {
                    return Err("source FIFO activation claim was not fresh acceptance".into());
                };
                let write = if index == 1 {
                    let batch = claim.batch().ok_or("source FIFO activation has no batch")?;
                    let item = batch
                        .batch()
                        .items()
                        .first()
                        .ok_or("empty source FIFO batch")?;
                    let write = item.plan().write().ok_or("missing source FIFO write")?;
                    let selected = (batch.batch().items().len(), write.record.id().as_bytes());
                    assert_eq!(selected, (1, ids[0]));
                    Some(write.record.clone())
                } else {
                    let empty = (true, SelectionStop::Complete);
                    assert_eq!((claim.batch().is_none(), claim.stop()), empty);
                    None
                };
                Ok((plan.control().next(), write))
            },
        )?;
        control = next_control;
        if let Some(record) = write {
            rows[0] = record;
        }
    }
    Ok(())
}

mod policy_activation;
