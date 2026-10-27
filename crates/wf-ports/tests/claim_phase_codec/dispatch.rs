//! Actual decoded inputs drive coherent pure apply and original outcome replay.

use std::collections::BTreeMap;

use wf_domain::claim::apply::{
    ClaimApplyView, ClaimDecision, ClaimEncoding, ClaimRejection, ClaimWorkLimits,
};
use wf_domain::claim::{ClaimRecord, DueOrder, ScopedRecordId, SelectionStop};
use wf_domain::history::EventKind;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::{ApplyError, GroupView};
use wf_domain::workflow::control::{
    ControlState, EntryIdentity, EntryStamp, PreparedDigest, plan_control,
};
use wf_domain::workflow::indexed::{IndexRows, IndexedGroupView};
use wf_domain::workflow::request::gate::GateView;
use wf_domain::workflow::request::{
    DispatchDecision, DispatchPlan, RetainedMetadata, RetainedOutcome, RetainedStatus,
    decide_claim_request,
};
use wf_domain::workflow::{
    CommittedCommand, Create, Operation, Outcome, WorkflowRecord, WorkflowStatus, reduce,
};
use wf_ports::consensus::PreparedOperation;
use wf_ports::log::{CodecLimits, DecodedRequest, decode_prepared, encode_prepared};
use wf_types::identity::{PartitionId, RecordId};
use wf_types::version::{
    AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity, TimestampMillis,
};

use super::{TestResult, request};

fn initial_control() -> TestResult<ControlState> {
    Ok(plan_control(
        ControlState::initial(CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?),
        EntryIdentity::new(
            LogIdentity::new(1, 7, AppliedPosition::new(0)),
            PreparedDigest::new([17; 32]),
        ),
        EntryStamp::Unstamped,
    )?
    .next())
}

fn fixture_record() -> TestResult<WorkflowRecord> {
    let command = CommittedCommand {
        id: RecordId::new(vec![0, 255])?,
        now: TimestampMillis::new(5),
        operation: Operation::Create(Create {
            workflow_type: vec![b't', 255],
            state: b"waiting".to_vec(),
            run_at: Some(TimestampMillis::new(9)),
            payload: Some(vec![0, 1, 255]),
        }),
    };
    Ok(reduce(None, &command)?
        .into_parts()
        .1
        .ok_or("missing fixture record")?
        .record)
}

fn with_parallel_view<T>(
    entries: &[ClaimRecord<'_>],
    partitions: &[PartitionId],
    gate: GateView<'_>,
    run: impl for<'view> FnOnce(ClaimApplyView<'view>) -> TestResult<T>,
) -> TestResult<T> {
    if entries.len() > 1 || partitions.len() > 1 {
        return Err("fixture accepts at most one record and partition".into());
    }
    let placements: BTreeMap<_, _> = partitions
        .iter()
        .map(|partition| (partition.clone(), gate.scope.group().clone()))
        .collect();
    let records = entries
        .iter()
        .map(|entry| (ScopedRecordId::for_record(entry), *entry))
        .collect();
    let canonical = entries
        .iter()
        .map(|entry| (ScopedRecordId::for_record(entry), entry.record))
        .collect();
    let mut parallel = BTreeMap::new();
    let mut keys = Vec::new();
    for entry in entries {
        parallel.insert(
            DueOrder::for_record(entry).ok_or("missing fixture due row")?,
            *entry,
        );
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
            .find(|partition| partition.as_bytes() == entry.partition_key)
            .ok_or("missing fixture partition")?;
        assert_eq!(placements.get(partition), Some(gate.scope.group()));
        assert_eq!(key, b"f:{p}:s:\x00\xFF");
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
            fifo: &BTreeMap::new(),
            policies: &BTreeMap::new(),
        },
        revision: 1,
        encodings: &encodings,
    })
}

fn assert_claim_plan(
    plan: &DispatchPlan<'_, ClaimDecision<'_>>,
    original: &WorkflowRecord,
    partition: &PartitionId,
) -> TestResult {
    let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision() else {
        return Err("expected fresh accepted claim".into());
    };
    assert!(plan.requires_outcome_write());
    assert_eq!(plan.control().next().position().get(), 1);
    assert_eq!(
        plan.control().next().time().hlc(),
        HlcTimestamp::new(11, 2)?
    );
    assert_eq!(plan.control().next().time().wall().get(), 7);
    assert_eq!(plan.control().execution_time().get(), 11);
    assert_eq!((claim.scanned(), claim.head_reads()), (1, 0));
    assert_eq!(claim.stop(), SelectionStop::Complete);
    let batch = claim.batch().ok_or("missing claim mutation")?;
    assert_eq!(batch.batch().items().len(), 1);
    let item = &batch.batch().items()[0];
    assert_eq!(item.partition(), partition);
    assert_eq!(item.plan().outcome(), Outcome::Changed);
    let write = item.plan().write().ok_or("missing record write")?;
    assert_eq!(write.expected_version, Some(1));
    let record = &write.record;
    assert_claim_record(record, original)?;
    assert_eq!(item.plan().history().iter().len(), 1);
    let history = item.plan().history().ok_or("missing claim history")?;
    assert_claim_history(history);
    let metadata = batch.metadata().collect::<Vec<_>>();
    assert_eq!(metadata.len(), 1);
    assert_eq!((metadata[0].priority, metadata[0].state_enter_seq), (2, 11));
    assert_eq!(batch.consumed_ordinals(), 0);
    assert_eq!(batch.next_ordinal(), Some(u32::MAX));
    let deltas = batch.deltas().collect::<Vec<_>>();
    assert_eq!(deltas.len(), 1);
    let IndexRows::Parallel { due: before } = deltas[0].before.ok_or("missing old index row")?
    else {
        return Err("wrong old index mode".into());
    };
    assert_eq!(
        before,
        DueOrder {
            priority: 2,
            due: TimestampMillis::new(9),
            id: &[0, 255],
            partition_key: b"p",
            state: b"waiting",
            workflow_type: &[b't', 255],
        }
    );
    assert_eq!(
        deltas[0].after,
        Some(IndexRows::Parallel {
            due: DueOrder {
                due: TimestampMillis::new(61),
                ..before
            },
        })
    );
    Ok(())
}

#[test]
fn actual_decoded_claim_selects_coherent_record_and_binds_lease_history_and_indexes() -> TestResult
{
    let expected = request("claim_any")?;
    let encoded = encode_prepared(&expected, CodecLimits::default())?;
    let decoded = decode_prepared(encoded.as_bytes(), CodecLimits::default())?;
    let record = fixture_record()?;
    let before = record.clone();
    let partition = PartitionId::new(b"p".to_vec())?;
    let entries = [ClaimRecord {
        record: &record,
        partition_key: partition.as_bytes(),
        priority: 2,
        state_enter_seq: 11,
    }];
    let outcomes = BTreeMap::new();
    let control = initial_control()?;
    with_parallel_view(
        &entries,
        std::slice::from_ref(&partition),
        GateView {
            scope: &decoded.request().scope,
            control,
            outcomes: &outcomes,
        },
        |view| {
            let envelope = decoded
                .claim_envelope(LogIdentity::new(1, 7, AppliedPosition::new(1)))
                .ok_or("missing claim envelope")?;
            let plan = decide_claim_request(
                view,
                &envelope,
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            assert_claim_plan(&plan, &record, &partition)
        },
    )?;
    assert_eq!(record, before);
    assert_eq!(control.position().get(), 0);
    assert!(outcomes.is_empty());
    Ok(())
}

#[test]
fn actual_decoded_empty_claim_is_complete_and_finalizes_request() -> TestResult {
    let expected = request("claim_any")?;
    let encoded = encode_prepared(&expected, CodecLimits::default())?;
    let decoded = decode_prepared(encoded.as_bytes(), CodecLimits::default())?;
    let outcomes = BTreeMap::new();
    let control = initial_control()?;
    with_parallel_view(
        &[],
        &[],
        GateView {
            scope: &decoded.request().scope,
            control,
            outcomes: &outcomes,
        },
        |view| {
            let envelope = decoded
                .claim_envelope(LogIdentity::new(1, 7, AppliedPosition::new(1)))
                .ok_or("missing empty envelope")?;
            let plan = decide_claim_request(
                view,
                &envelope,
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision() else {
                return Err("expected accepted empty claim".into());
            };
            assert!(claim.batch().is_none());
            assert_eq!((claim.scanned(), claim.head_reads()), (0, 0));
            assert_eq!(claim.stop(), SelectionStop::Complete);
            assert!(plan.requires_outcome_write());
            assert_eq!(plan.control().next().position().get(), 1);
            assert_eq!(
                plan.control().next().time().hlc(),
                HlcTimestamp::new(11, 2)?
            );
            assert_eq!(plan.control().next().time().wall().get(), 7);
            Ok(())
        },
    )?;
    assert_eq!(control.position().get(), 0);
    assert!(outcomes.is_empty());
    Ok(())
}

#[test]
fn actual_decoded_claim_rejects_fresh_invalid_priority_without_mutation() -> TestResult {
    let expected = request("claim_priority_max")?;
    let encoded = encode_prepared(&expected, CodecLimits::default())?;
    let decoded = decode_prepared(encoded.as_bytes(), CodecLimits::default())?;
    let outcomes = BTreeMap::new();
    let control = initial_control()?;
    with_parallel_view(
        &[],
        &[],
        GateView {
            scope: &decoded.request().scope,
            control,
            outcomes: &outcomes,
        },
        |view| {
            let envelope = decoded
                .claim_envelope(LogIdentity::new(1, 7, AppliedPosition::new(1)))
                .ok_or("missing invalid-priority envelope")?;
            let plan = decide_claim_request(
                view,
                &envelope,
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            assert!(matches!(
                plan.decision(),
                DispatchDecision::Fresh(ClaimDecision::Rejected(ClaimRejection::InvalidPriority))
            ));
            assert!(plan.requires_outcome_write());
            assert_eq!(plan.control().next().position().get(), 1);
            assert_eq!(
                plan.control().next().time().hlc(),
                HlcTimestamp::new(11, 2)?
            );
            assert_eq!(plan.control().next().time().wall().get(), 7);
            assert!(view.group.records.is_empty() && view.group.parallel.is_empty());
            Ok(())
        },
    )?;
    assert_eq!(control.position().get(), 0);
    assert!(outcomes.is_empty());
    Ok(())
}

#[test]
fn actual_decoded_unsupported_claim_token_exposes_no_control_or_outcome_plan() -> TestResult {
    let expected = request("claim_token_zero")?;
    let encoded = encode_prepared(&expected, CodecLimits::default())?;
    let decoded = decode_prepared(encoded.as_bytes(), CodecLimits::default())?;
    let outcomes = BTreeMap::new();
    let control = initial_control()?;
    with_parallel_view(
        &[],
        &[],
        GateView {
            scope: &decoded.request().scope,
            control,
            outcomes: &outcomes,
        },
        |view| {
            let envelope = decoded
                .claim_envelope(LogIdentity::new(1, 7, AppliedPosition::new(1)))
                .ok_or("missing unsupported-token envelope")?;
            assert!(matches!(
                decide_claim_request(
                    view,
                    &envelope,
                    AdmissionLimits::default(),
                    ClaimWorkLimits::default()
                ),
                Err(ApplyError::InvalidAuthority)
            ));
            Ok(())
        },
    )?;
    assert_eq!(control.position().get(), 0);
    assert!(outcomes.is_empty());
    Ok(())
}

fn first_claim_status(
    decoded: &DecodedRequest<'_>,
    name: &str,
    log: LogIdentity,
) -> TestResult<(ControlState, RetainedStatus)> {
    let outcomes = BTreeMap::new();
    with_parallel_view(
        &[],
        &[],
        GateView {
            scope: &decoded.request().scope,
            control: initial_control()?,
            outcomes: &outcomes,
        },
        |view| {
            let envelope = decoded
                .claim_envelope(log)
                .ok_or("missing first envelope")?;
            let first = decide_claim_request(
                view,
                &envelope,
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            let status = match first.decision() {
                DispatchDecision::Fresh(ClaimDecision::Accepted(_)) => RetainedStatus::Accepted,
                DispatchDecision::Fresh(ClaimDecision::Rejected(error)) => {
                    RetainedStatus::ClaimRejected(*error)
                }
                _ => return Err("expected actual fresh first processing".into()),
            };
            assert_eq!(
                status,
                if name == "claim_any" {
                    RetainedStatus::Accepted
                } else {
                    RetainedStatus::ClaimRejected(ClaimRejection::InvalidPriority)
                }
            );
            assert!(first.requires_outcome_write());
            Ok((first.control().next(), status))
        },
    )
}

#[test]
fn actual_decoded_retained_claim_replays_before_changed_server_phase_inputs() -> TestResult {
    for name in ["claim_any", "claim_priority_max"] {
        let expected = request(name)?;
        let encoded = encode_prepared(&expected, CodecLimits::default())?;
        let decoded = decode_prepared(encoded.as_bytes(), CodecLimits::default())?;
        let log = LogIdentity::new(1, 7, AppliedPosition::new(1));
        let (control, status) = first_claim_status(&decoded, name, log)?;
        let original = RetainedOutcome::new(
            RetainedMetadata {
                scope: expected.scope.clone(),
                request: expected.request.clone(),
                intent: expected.intent,
                original_position: log,
                original_time: TimestampMillis::new(11),
                expires_at: TimestampMillis::new(100),
                status,
            },
            1,
            vec![0, 255, 7],
            AdmissionLimits::default(),
        )?;
        let outcomes = BTreeMap::from([(expected.request.clone(), original.clone())]);
        let mut retry = expected.clone();
        let PreparedOperation::ClaimPhase(phase) = &mut retry.operation else {
            return Err("missing retry phase".into());
        };
        phase.token_version = 0;
        phase.scan_budget = 0;
        phase.head_budget = 0;
        phase.first_ordinal = 0;
        assert!(phase.query == super::phase(name)?.query);
        assert_eq!(retry.intent, expected.intent);
        let retry_bytes = encode_prepared(&retry, CodecLimits::default())?;
        assert_ne!(retry_bytes.as_bytes(), encoded.as_bytes());
        assert_ne!(retry_bytes.digest(), encoded.digest());
        let retry_decoded = decode_prepared(retry_bytes.as_bytes(), CodecLimits::default())?;
        with_parallel_view(
            &[],
            &[],
            GateView {
                scope: &retry_decoded.request().scope,
                control,
                outcomes: &outcomes,
            },
            |view| {
                let envelope = retry_decoded
                    .claim_envelope(LogIdentity::new(2, 9, AppliedPosition::new(2)))
                    .ok_or("missing retry envelope")?;
                let replay = decide_claim_request(
                    view,
                    &envelope,
                    AdmissionLimits::default(),
                    ClaimWorkLimits::default(),
                )?;
                let DispatchDecision::Replay(retained) = replay.decision() else {
                    return Err("expected retained original claim".into());
                };
                assert_eq!(*retained, &original);
                assert_eq!(
                    retained.response().as_ptr(),
                    outcomes[&expected.request].response().as_ptr()
                );
                assert_eq!(retained.status(), status);
                assert_eq!(retained.original_position(), log);
                assert_eq!(retained.original_time().get(), 11);
                assert_eq!(retained.expires_at().get(), 100);
                assert_eq!(replay.control().next().position().get(), 2);
                assert_eq!(
                    replay.control().next().time().hlc(),
                    HlcTimestamp::new(11, 2)?
                );
                assert_eq!(replay.control().next().time().wall().get(), 7);
                assert!(!replay.requires_outcome_write());
                assert!(view.group.records.is_empty() && view.group.parallel.is_empty());
                Ok(())
            },
        )?;
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[&expected.request], original);
        assert_eq!(control.position().get(), 1);
    }
    Ok(())
}

fn assert_claim_record(record: &WorkflowRecord, original: &WorkflowRecord) -> TestResult {
    assert_eq!(record.id(), original.id());
    assert_eq!(record.workflow_type(), original.workflow_type());
    assert_eq!(record.payload(), Some([0, 1, 255].as_slice()));
    assert_eq!(record.created_at().get(), 5);
    assert_eq!(record.updated_at().get(), 11);
    assert_eq!(
        (record.version(), record.fence().get(), record.attempts()),
        (2, 1, 0)
    );
    assert_eq!(record.status(), WorkflowStatus::Running);
    assert_eq!(record.state(), b"running");
    assert_eq!(record.run_state(), Some(b"waiting".as_slice()));
    assert_eq!(record.next_run_at(), Some(TimestampMillis::new(61)));
    let lease = record.lease().ok_or("missing lease")?;
    assert_eq!(lease.worker, [b'w', 255]);
    assert_eq!(lease.token.as_bytes(), b"w\xFF:11:1");
    assert_eq!(lease.deadline.get(), 61);
    Ok(())
}

fn assert_claim_history(history: &wf_domain::history::HistoryEntry) {
    assert_eq!(history.event_id, "11-2");
    assert_eq!(history.kind, EventKind::Claimed);
    assert_eq!(
        (
            history.occurred_at.get(),
            history.version,
            history.fence.get()
        ),
        (11, 2, 1)
    );
    assert_eq!(history.attempts, 0);
    assert_eq!(history.state, b"running");
    assert_eq!(history.worker.as_deref(), Some([b'w', 255].as_slice()));
    assert_eq!(history.lease_deadline.get(), 61);
    assert_eq!(history.next_run_at, Some(TimestampMillis::new(61)));
    assert!(history.retry.is_none());
}
