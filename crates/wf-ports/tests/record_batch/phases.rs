//! Bounded raw preparation and one retained gate before complete current authority.

use std::collections::BTreeMap;
use std::mem::size_of;

use wf_domain::claim::Claim;
use wf_domain::history::EventKind;
use wf_domain::lease::LeaseToken;
use wf_domain::policy::InstalledRetryPolicy;
use wf_domain::workflow::Operation;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::{ApplyError, GroupView};
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::indexed::{IndexRejection, IndexRows, IndexedDecision, IndexedGroupView};
use wf_domain::workflow::request::gate::GateView;
use wf_domain::workflow::request::indexed::IndexedRequestView;
use wf_domain::workflow::request::{ClientIntent, RetainedOutcome, RetainedStatus};
use wf_ports::blob::RecordBatchValueEffects;
use wf_ports::consensus::PreparedOperation;
use wf_ports::log::{
    CodecError, CodecLimits, DecodedRequest, FreshRecordBatch, RecordBatchApplyDecision,
    RecordBatchApplyError, RecordBatchInspection, decide_fresh_record_batch, decide_record_batch,
    decode_prepared, encode_prepared, encode_record_batch, inspect_record_batch,
    prepare_record_batch,
};
use wf_types::identity::{GroupId, PartitionId, RequestId};
use wf_types::version::{Fence, TimestampMillis};

use super::{
    MIN_FRAME, TestResult, WORKSPACE, clock, create, hex, limits, log, request, scope, with_empty,
};

// Independently packed before phase implementation: intent57 and frame120.
const VALUE_FRAME: &str = "4657435000010100000001740000000167000000017a00000001710005ad6eeaaf282d9eff4643ae161d860eba1a40f3c35f472482292fd2ff05bd812e0000000000000001000000000000000000010000000100000001700000000169000000000000000001010000000172000000017100010000000176";
const MIN_D: usize = 16_780_748;

fn original(rejected: bool) -> TestResult<(ControlState, RetainedOutcome)> {
    let mut item = create(b"i")?;
    item.create_priority = if rejected { 255 } else { 0 };
    let frame = encode_record_batch(request(vec![item])?, limits(), WORKSPACE)?;
    let mut saved = None;
    with_empty(frame.as_bytes(), limits(), WORKSPACE, |result| {
        let (control, decision) = result?.into_parts();
        let RecordBatchApplyDecision::Fresh { operation, outcome } = decision else {
            return Err("original was not fresh".into());
        };
        if rejected {
            assert_eq!(
                operation,
                IndexedDecision::Rejected(IndexRejection::InvalidPriority)
            );
            assert_eq!(outcome.response(), [1, 4, 0]);
        } else {
            assert!(matches!(operation, IndexedDecision::Accepted(_)));
            assert_eq!(outcome.response(), [0]);
        }
        saved = Some((control.next(), *outcome));
        Ok(())
    })?;
    saved.ok_or_else(|| "original outcome absent".into())
}

fn fresh_phase(encoded: &[u8], workspace: usize) -> TestResult<FreshRecordBatch<'_>> {
    let query = scope()?;
    let outcomes = BTreeMap::new();
    let decoded = prepare_record_batch(encoded, limits(), workspace)?;
    let inspected = inspect_record_batch(
        decoded,
        log(0),
        GateView {
            scope: &query,
            control: ControlState::initial(clock(0)?),
            outcomes: &outcomes,
        },
        limits(),
        workspace,
    )?;
    let RecordBatchInspection::Fresh(fresh) = inspected else {
        return Err("new request was not fresh".into());
    };
    // Fresh has no retained-map or expected-query lifetime.
    drop(query);
    drop(outcomes);
    Ok(fresh)
}

fn assert_fresh_error(
    fresh: FreshRecordBatch<'_>,
    foreign_group: bool,
    expected: RecordBatchApplyError,
) -> TestResult {
    let query = scope()?;
    let group = if foreign_group {
        GroupId::new(b"foreign".to_vec())?
    } else {
        query.group().clone()
    };
    let placements = BTreeMap::from([(PartitionId::new(b"p".to_vec())?, group.clone())]);
    let current = BTreeMap::new();
    let records = BTreeMap::new();
    let parallel = BTreeMap::new();
    let fifo = BTreeMap::new();
    let policies = BTreeMap::new();
    let retries: BTreeMap<&[u8], &InstalledRetryPolicy> = BTreeMap::new();
    let view = IndexedGroupView {
        batch: GroupView {
            group: &group,
            placements: &placements,
            records: &current,
        },
        records: &records,
        parallel: &parallel,
        fifo: &fifo,
        policies: &policies,
    };
    assert_eq!(
        decide_fresh_record_batch(fresh, view, &retries, 100).err(),
        Some(expected)
    );
    Ok(())
}

#[test]
fn bounded_record_batch_prepare_admits_before_decode() -> TestResult {
    let encoded = hex(MIN_FRAME)?;
    assert_eq!(encoded.len(), 115);
    for maximum in [0, MIN_D - 1] {
        assert!(matches!(
            prepare_record_batch(&encoded, limits(), maximum),
            Err(RecordBatchApplyError::Workspace { attempted: MIN_D, maximum: cap }) if cap == maximum
        ));
    }
    let fresh = fresh_phase(&encoded, MIN_D)?;
    assert_eq!(fresh.control().execution_time(), TimestampMillis::new(1));
    assert_fresh_error(
        fresh,
        false,
        RecordBatchApplyError::Workspace {
            attempted: 33_558_412,
            maximum: MIN_D,
        },
    )?;
    let mut trailing = encoded.clone();
    trailing.push(0);
    assert!(matches!(
        prepare_record_batch(&trailing, limits(), 0),
        Err(RecordBatchApplyError::Encoding(CodecError::TrailingBytes))
    ));
    let mut malformed = encoded;
    malformed[0] = 0;
    assert!(matches!(
        prepare_record_batch(&malformed, limits(), 0),
        Err(RecordBatchApplyError::Encoding(CodecError::Malformed))
    ));
    Ok(())
}

fn assert_complete_class(
    control: ControlState,
    outcomes: &BTreeMap<RequestId, RetainedOutcome>,
    rejected: bool,
    class: u8,
) -> TestResult {
    let query = scope()?;
    let original = outcomes.values().next().ok_or("outcome absent")?;
    let mut item = create(b"i")?;
    item.create_priority = if rejected { 255 } else { 0 };
    item.command.now = TimestampMillis::new(if class == 3 { 1 } else { 999 });
    if class == 1 {
        let Operation::Create(create) = &mut item.command.operation else {
            return Err("not create".into());
        };
        create.payload = Some(Vec::new());
    }
    let mut value = request(vec![item])?;
    value.stamp = clock(match class {
        2 => 101,
        3 => 1,
        _ => 5,
    })?;
    let encoded = encode_record_batch(value, limits(), WORKSPACE)?;
    let workspace = if class == 1 { 16_780_764 } else { MIN_D };
    let workspace = workspace + size_of::<Option<RecordBatchValueEffects>>();
    let decoded = prepare_record_batch(encoded.as_bytes(), limits(), workspace)?;
    let inspected = inspect_record_batch(
        decoded,
        log(u64::from(class != 3)),
        GateView {
            scope: &query,
            control,
            outcomes,
        },
        limits(),
        workspace,
    )?;
    let RecordBatchInspection::Complete(plan) = inspected else {
        return Err("retained request required current authority".into());
    };
    let plan: wf_ports::log::RecordBatchApplyPlan<'_> = plan;
    drop(encoded);
    match class {
        0 => {
            let RecordBatchApplyDecision::Replay(replayed) = plan.decision() else {
                return Err("not replay".into());
            };
            assert_eq!(
                std::ptr::from_ref::<RetainedOutcome>(replayed),
                std::ptr::from_ref(original)
            );
            assert_eq!(
                replayed.response(),
                if rejected { &[1, 4, 0][..] } else { &[0][..] }
            );
            assert_eq!(
                replayed.status(),
                if rejected {
                    RetainedStatus::IndexedRejected(IndexRejection::InvalidPriority)
                } else {
                    RetainedStatus::Accepted
                }
            );
            assert_eq!(replayed.original_position(), log(0));
            assert_eq!(replayed.original_time(), TimestampMillis::new(1));
            assert_eq!(replayed.expires_at(), TimestampMillis::new(101));
            assert_eq!(replayed.response_version(), 1);
        }
        1 => assert!(matches!(
            plan.decision(),
            RecordBatchApplyDecision::IntentConflict
        )),
        2 => assert!(matches!(plan.decision(), RecordBatchApplyDecision::Expired)),
        3 => {
            assert!(matches!(
                plan.decision(),
                RecordBatchApplyDecision::LogReplay
            ));
            assert!(plan.control().is_log_replay());
            assert_eq!(plan.control().next(), control);
        }
        _ => return Err("unknown gate class".into()),
    }
    if class != 3 {
        assert_eq!(plan.control().next().position(), log(1).index());
        assert_eq!(
            plan.control().execution_time(),
            TimestampMillis::new(if class == 2 { 101 } else { 5 })
        );
    }
    Ok(())
}

#[test]
fn record_batch_inspection_finishes_all_replay_classes() -> TestResult {
    for rejected in [false, true] {
        let (control, outcome) = original(rejected)?;
        let outcomes = BTreeMap::from([(outcome.request().clone(), outcome)]);
        for class in 0..4 {
            assert_complete_class(control, &outcomes, rejected, class)?;
        }
    }
    Ok(())
}

fn assert_cached_derivation_error() -> TestResult {
    let mut item = create(b"i")?;
    item.command.operation = Operation::Claim(Claim {
        expected_state: b"q".to_vec(),
        worker: b"w".to_vec(),
        token: LeaseToken::new(b"tok".to_vec())?,
        lease_ms: 1,
    });
    let mut value = request(vec![item])?;
    value.intent = ClientIntent::record_batch([0x22; 32]);
    let frame = encode_prepared(&value, limits())?;
    // Generic Batch5 Claim decoding succeeds; private batch intent derivation rejects it.
    let decoded = decode_prepared(frame.as_bytes(), limits())?;
    assert_eq!(decoded.request().intent.version(), 5);
    assert!(matches!(
        prepare_record_batch(frame.as_bytes(), limits(), WORKSPACE),
        Err(RecordBatchApplyError::Encoding(CodecError::UnsupportedTag))
    ));
    let query = scope()?;
    let outcomes = BTreeMap::new();
    assert!(matches!(
        inspect_record_batch(
            decoded,
            log(0),
            GateView {
                scope: &query,
                control: ControlState::initial(clock(0)?),
                outcomes: &outcomes
            },
            limits(),
            WORKSPACE
        ),
        Err(RecordBatchApplyError::Encoding(CodecError::UnsupportedTag))
    ));
    Ok(())
}

#[test]
fn record_batch_inspection_rejects_forged_intent_and_profile() -> TestResult {
    let mut forged = hex(MIN_FRAME)?;
    forged[29] ^= 1;
    assert!(matches!(
        prepare_record_batch(&forged, limits(), WORKSPACE),
        Err(RecordBatchApplyError::IntentMismatch)
    ));
    let query = scope()?;
    let outcomes = BTreeMap::new();
    let gate = GateView {
        scope: &query,
        control: ControlState::initial(clock(0)?),
        outcomes: &outcomes,
    };
    let generic = decode_prepared(&forged, limits())?;
    assert!(matches!(
        inspect_record_batch(generic, log(0), gate, limits(), WORKSPACE),
        Err(RecordBatchApplyError::IntentMismatch)
    ));
    let valid = hex(MIN_FRAME)?;
    let distinct = CodecLimits::new(AdmissionLimits::new(1, 16 * 1_024 * 1_024, 1_000)?);
    assert!(matches!(
        inspect_record_batch(
            decode_prepared(&valid, limits())?,
            log(0),
            gate,
            distinct,
            WORKSPACE
        ),
        Err(RecordBatchApplyError::Apply(ApplyError::InvalidAuthority))
    ));
    assert!(matches!(
        inspect_record_batch(
            decode_prepared(&valid, limits())?,
            log(0),
            gate,
            limits(),
            WORKSPACE
        )?,
        RecordBatchInspection::Fresh(_)
    ));
    let old = encode_prepared(&request(vec![create(b"i")?])?, limits())?;
    assert!(matches!(
        prepare_record_batch(old.as_bytes(), limits(), WORKSPACE),
        Err(RecordBatchApplyError::Encoding(
            CodecError::UnsupportedVersion
        ))
    ));
    assert!(matches!(
        inspect_record_batch(
            decode_prepared(old.as_bytes(), limits())?,
            log(0),
            gate,
            limits(),
            WORKSPACE
        ),
        Err(RecordBatchApplyError::Encoding(
            CodecError::UnsupportedVersion
        ))
    ));
    assert_cached_derivation_error()
}

fn payload_address(decoded: &DecodedRequest<'_>) -> TestResult<*const u8> {
    let PreparedOperation::Batch(items) = &decoded.request().operation else {
        return Err("not batch".into());
    };
    let Operation::Create(create) = &items[0].command.operation else {
        return Err("not create".into());
    };
    let payload = create.payload.as_ref().ok_or("payload absent")?;
    assert_eq!(payload, b"v");
    Ok(payload.as_ptr())
}

#[test]
fn record_batch_fresh_keeps_decoded_owner_and_complete_authority() -> TestResult {
    let encoded = hex(VALUE_FRAME)?;
    assert_eq!(encoded.len(), 120);
    let fresh = {
        let query = scope()?;
        let outcomes = BTreeMap::new();
        let decoded = prepare_record_batch(&encoded, limits(), WORKSPACE)?;
        let before = payload_address(&decoded)?;
        let RecordBatchInspection::Fresh(fresh) = inspect_record_batch(
            decoded,
            log(0),
            GateView {
                scope: &query,
                control: ControlState::initial(clock(0)?),
                outcomes: &outcomes,
            },
            limits(),
            WORKSPACE,
        )?
        else {
            return Err("new request was not fresh".into());
        };
        assert_eq!(payload_address(fresh.decoded())?, before);
        drop(query);
        drop(outcomes);
        fresh
    };
    let query = scope()?;
    let placements = BTreeMap::from([(PartitionId::new(b"p".to_vec())?, query.group().clone())]);
    let current = BTreeMap::new();
    let records = BTreeMap::new();
    let parallel = BTreeMap::new();
    let fifo = BTreeMap::new();
    let policies = BTreeMap::new();
    let retries = BTreeMap::new();
    let view = IndexedGroupView {
        batch: GroupView {
            group: query.group(),
            placements: &placements,
            records: &current,
        },
        records: &records,
        parallel: &parallel,
        fifo: &fifo,
        policies: &policies,
    };
    let plan = decide_fresh_record_batch(fresh, view, &retries, 100)?;
    drop(encoded);
    let RecordBatchApplyDecision::Fresh {
        operation: IndexedDecision::Accepted(batch),
        outcome,
    } = plan.decision()
    else {
        return Err("fresh creation not accepted".into());
    };
    let item = &batch.batch().items()[0];
    let record = &item.plan().write().ok_or("write absent")?.record;
    assert_eq!(record.id().as_bytes(), b"i");
    assert_eq!(record.workflow_type(), b"r");
    assert_eq!(record.state(), b"q");
    assert_eq!(record.payload(), Some(b"v".as_slice()));
    assert_eq!(record.version(), 1);
    assert_eq!(record.fence(), Fence::new(0));
    let history = item.plan().history().ok_or("history absent")?;
    assert_eq!(history.kind, EventKind::Created);
    assert_eq!(history.version, 1);
    assert_eq!(history.fence, Fence::new(0));
    let metadata = batch.metadata().next().ok_or("ordering absent")?;
    assert_eq!(metadata.priority, 0);
    assert_eq!(metadata.state_enter_seq, 0);
    let delta = batch.deltas().next().ok_or("index delta absent")?;
    assert_eq!(delta.before, None);
    let Some(IndexRows::Parallel { due }) = delta.after else {
        return Err("parallel row absent".into());
    };
    assert_eq!(
        (due.id, due.partition_key, due.state, due.workflow_type),
        (
            b"i".as_slice(),
            b"p".as_slice(),
            b"q".as_slice(),
            b"r".as_slice()
        )
    );
    assert_eq!(due.due, TimestampMillis::new(1));
    assert_eq!(outcome.response(), [0]);
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.original_position(), log(0));
    assert_eq!(outcome.original_time(), TimestampMillis::new(1));
    assert_eq!(outcome.expires_at(), TimestampMillis::new(101));
    Ok(())
}

fn assert_legacy_group_error(
    encoded: &[u8],
    control: ControlState,
    outcomes: &BTreeMap<RequestId, RetainedOutcome>,
    expected: RecordBatchApplyError,
) -> TestResult {
    let query = scope()?;
    let foreign = GroupId::new(b"foreign".to_vec())?;
    let placements = BTreeMap::new();
    let current = BTreeMap::new();
    let records = BTreeMap::new();
    let parallel = BTreeMap::new();
    let fifo = BTreeMap::new();
    let policies = BTreeMap::new();
    let retries = BTreeMap::new();
    let view = IndexedRequestView {
        gate: GateView {
            scope: &query,
            control,
            outcomes,
        },
        group: IndexedGroupView {
            batch: GroupView {
                group: &foreign,
                placements: &placements,
                records: &current,
            },
            records: &records,
            parallel: &parallel,
            fifo: &fifo,
            policies: &policies,
        },
    };
    assert_eq!(
        decide_record_batch(encoded, log(1), view, &retries, 0, limits(), WORKSPACE).err(),
        Some(expected)
    );
    Ok(())
}

#[test]
fn record_batch_phases_preserve_legacy_error_order() -> TestResult {
    let (control, outcome) = original(false)?;
    let outcomes = BTreeMap::from([(outcome.request().clone(), outcome)]);
    let valid = hex(MIN_FRAME)?;
    let mut forged = valid.clone();
    forged[29] ^= 1;
    assert_legacy_group_error(
        &forged,
        control,
        &outcomes,
        RecordBatchApplyError::IntentMismatch,
    )?;
    assert_legacy_group_error(
        &valid,
        control,
        &outcomes,
        RecordBatchApplyError::Apply(ApplyError::InvalidAuthority),
    )?;
    let query = scope()?;
    let RecordBatchInspection::Complete(plan) = inspect_record_batch(
        prepare_record_batch(&valid, limits(), WORKSPACE)?,
        log(1),
        GateView {
            scope: &query,
            control,
            outcomes: &outcomes,
        },
        limits(),
        WORKSPACE,
    )?
    else {
        return Err("replay required a current group".into());
    };
    assert!(matches!(
        plan.decision(),
        RecordBatchApplyDecision::Replay(_)
    ));
    assert_fresh_error(
        fresh_phase(&valid, WORKSPACE)?,
        true,
        RecordBatchApplyError::Apply(ApplyError::InvalidAuthority),
    )?;
    let mut wrong_time = hex(VALUE_FRAME)?;
    wrong_time[94..102].copy_from_slice(&999_u64.to_be_bytes());
    assert_fresh_error(
        fresh_phase(&wrong_time, WORKSPACE)?,
        false,
        RecordBatchApplyError::Apply(ApplyError::InvalidAuthority),
    )?;
    Ok(())
}
