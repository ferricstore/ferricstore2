//! A truthful retained point/cohort observation; no current workflow authority.

use std::collections::BTreeMap;
use std::mem::size_of;

use wf_domain::workflow::Operation;
use wf_domain::workflow::batch::{ApplyError, GroupView};
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::indexed::IndexedGroupView;
use wf_domain::workflow::request::gate::{GateView, PointGateView};
use wf_domain::workflow::request::indexed::IndexedRequestView;
use wf_domain::workflow::request::{RetainedMetadata, RetainedOutcome};
use wf_ports::blob::RecordBatchValueEffects;
use wf_ports::consensus::PreparedOperation;
use wf_ports::log::{
    CodecLimits, DecodedRequest, RecordBatchApplyDecision, RecordBatchApplyError,
    RecordBatchApplyPlan, RecordBatchInspection, decide_fresh_record_batch, decide_record_batch,
    encode_record_batch, inspect_record_batch_point, prepare_record_batch,
};
use wf_types::identity::{PartitionId, RequestId};
use wf_types::version::TimestampMillis;

use super::{
    MIN_FRAME, TestResult, WORKSPACE, clock, create, hex, limits, log, request, scope, with_empty,
};

const DECODE: usize = 16_780_748;
// Independently packed before phase implementation: the entire nonempty-v frame120.
const VALUE_FRAME: &str = "4657435000010100000001740000000167000000017a00000001710005ad6eeaaf282d9eff4643ae161d860eba1a40f3c35f472482292fd2ff05bd812e0000000000000001000000000000000000010000000100000001700000000169000000000000000001010000000172000000017100010000000176";

fn original(rejected: bool) -> TestResult<(ControlState, RetainedOutcome)> {
    let mut item = create(b"i")?;
    item.create_priority = if rejected { 255 } else { 0 };
    let frame = encode_record_batch(request(vec![item])?, limits(), WORKSPACE)?;
    let mut saved = None;
    with_empty(frame.as_bytes(), limits(), WORKSPACE, |result| {
        let (control, decision) = result?.into_parts();
        let RecordBatchApplyDecision::Fresh { outcome, .. } = decision else {
            return Err("original not fresh".into());
        };
        saved = Some((control.next(), *outcome));
        Ok(())
    })?;
    saved.ok_or_else(|| "original absent".into())
}

fn complete_class(control: ControlState, original: &RetainedOutcome, class: u8) -> TestResult {
    let mut item = create(b"i")?;
    item.create_priority = if original.response() == [0] { 0 } else { 255 };
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
    let workspace = if class == 1 { 16_780_764 } else { DECODE };
    let workspace = workspace
        + size_of::<Option<RecordBatchValueEffects>>()
        + std::mem::size_of::<wf_ports::log::DecodedRequest<'static>>();
    let query = scope()?;
    let result = inspect_record_batch_point(
        prepare_record_batch(encoded.as_bytes(), limits(), workspace)?,
        log(u64::from(class != 3)),
        PointGateView {
            scope: &query,
            control,
            retained_nonempty: true,
            outcome: Some(original),
        },
        limits(),
        workspace,
    )?;
    let RecordBatchInspection::Complete(plan) = result else {
        return Err("retained point required current authority".into());
    };
    let plan: RecordBatchApplyPlan<'_> = plan;
    drop(query);
    drop(encoded);
    match class {
        0 => {
            let RecordBatchApplyDecision::Replay(replayed) = plan.decision() else {
                return Err("not original replay".into());
            };
            assert!(std::ptr::eq(*replayed, original));
            assert_eq!(replayed.original_position(), log(0));
            assert_eq!(replayed.original_time(), TimestampMillis::new(1));
            assert_eq!(replayed.expires_at(), TimestampMillis::new(101));
            assert_eq!(replayed.response(), original.response());
            assert_eq!(replayed.status(), original.status());
        }
        1 => assert!(matches!(
            plan.decision(),
            RecordBatchApplyDecision::IntentConflict
        )),
        2 => assert!(matches!(plan.decision(), RecordBatchApplyDecision::Expired)),
        3 => assert!(matches!(
            plan.decision(),
            RecordBatchApplyDecision::LogReplay
        )),
        _ => return Err("unknown class".into()),
    }
    assert_eq!(plan.control().is_log_replay(), class == 3);
    assert_eq!(
        plan.control().next().position(),
        log(u64::from(class != 3)).index()
    );
    Ok(())
}

#[test]
fn point_record_batch_gate_finishes_without_current_authority() -> TestResult {
    for rejected in [false, true] {
        let (control, original) = original(rejected)?;
        assert_eq!(
            original.response(),
            if rejected { &[1, 4, 0][..] } else { &[0][..] }
        );
        for class in 0..4 {
            complete_class(control, &original, class)?;
        }
    }
    Ok(())
}

#[test]
fn point_record_batch_gate_rejects_incoherent_cohort_and_identity() -> TestResult {
    let query = scope()?;
    let (control, original) = original(false)?;
    let encoded = hex(MIN_FRAME)?;
    let view = PointGateView {
        scope: &query,
        control,
        retained_nonempty: false,
        outcome: Some(&original),
    };
    let invalid = RecordBatchApplyError::Apply(ApplyError::InvalidAuthority);
    assert_eq!(
        inspect_record_batch_point(
            prepare_record_batch(&encoded, limits(), WORKSPACE)?,
            log(1),
            view,
            limits(),
            WORKSPACE,
        )
        .err(),
        Some(invalid)
    );
    assert_eq!(
        inspect_record_batch_point(
            prepare_record_batch(&encoded, limits(), WORKSPACE)?,
            log(0),
            PointGateView {
                control: ControlState::initial(clock(0)?),
                retained_nonempty: true,
                outcome: None,
                ..view
            },
            limits(),
            WORKSPACE,
        )
        .err(),
        Some(invalid)
    );
    let wrong = RetainedOutcome::new(
        RetainedMetadata {
            scope: query.clone(),
            request: RequestId::new(b"other".to_vec())?,
            intent: original.intent(),
            original_position: original.original_position(),
            original_time: original.original_time(),
            expires_at: original.expires_at(),
            status: original.status(),
        },
        1,
        original.response().to_vec(),
        limits().admission(),
    )?;
    assert_eq!(
        inspect_record_batch_point(
            prepare_record_batch(&encoded, limits(), WORKSPACE)?,
            log(1),
            PointGateView {
                retained_nonempty: true,
                outcome: Some(&wrong),
                ..view
            },
            limits(),
            WORKSPACE,
        )
        .err(),
        Some(invalid)
    );
    let mut forged = encoded.clone();
    forged[29] ^= 1;
    let decoded = wf_ports::log::decode_prepared(&forged, limits())?;
    assert_eq!(
        inspect_record_batch_point(decoded, log(1), view, limits(), WORKSPACE).err(),
        Some(RecordBatchApplyError::IntentMismatch)
    );
    let decoded = wf_ports::log::decode_prepared(&encoded, limits())?;
    assert_eq!(
        inspect_record_batch_point(decoded, log(1), view, limits(), DECODE - 1).err(),
        Some(RecordBatchApplyError::Workspace {
            attempted: DECODE,
            maximum: DECODE - 1
        })
    );
    let distinct = CodecLimits::new(wf_domain::workflow::admission::AdmissionLimits::new(
        1,
        16 * 1_024 * 1_024,
        1_000,
    )?);
    let decoded = wf_ports::log::decode_prepared(&encoded, limits())?;
    assert_eq!(
        inspect_record_batch_point(decoded, log(1), view, distinct, WORKSPACE).err(),
        Some(invalid)
    );
    Ok(())
}

fn payload<'view>(decoded: &'view DecodedRequest<'_>) -> TestResult<&'view [u8]> {
    let PreparedOperation::Batch(items) = &decoded.request().operation else {
        return Err("not batch".into());
    };
    let Operation::Create(create) = &items[0].command.operation else {
        return Err("not create".into());
    };
    Ok(create.payload.as_deref().ok_or("payload absent")?)
}

#[test]
fn point_record_batch_fresh_preserves_owner_and_legacy_gate() -> TestResult {
    let encoded = hex(VALUE_FRAME)?;
    let decoded = prepare_record_batch(&encoded, limits(), WORKSPACE)?;
    assert_eq!(payload(&decoded)?, b"v");
    let before = payload(&decoded)?.as_ptr();
    let query = scope()?;
    let RecordBatchInspection::Fresh(fresh) = inspect_record_batch_point(
        decoded,
        log(0),
        PointGateView {
            scope: &query,
            control: ControlState::initial(clock(0)?),
            retained_nonempty: false,
            outcome: None,
        },
        limits(),
        WORKSPACE,
    )?
    else {
        return Err("empty cohort not fresh".into());
    };
    drop(query);
    assert_eq!(payload(fresh.decoded())?.as_ptr(), before);
    let query = scope()?;
    let placements = BTreeMap::from([(PartitionId::new(b"p".to_vec())?, query.group().clone())]);
    let current = BTreeMap::new();
    let records = BTreeMap::new();
    let parallel = BTreeMap::new();
    let fifo = BTreeMap::new();
    let policies = BTreeMap::new();
    let retries = BTreeMap::new();
    let outcomes = BTreeMap::new();
    let group = IndexedGroupView {
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
    let legacy = decide_record_batch(
        &encoded,
        log(0),
        IndexedRequestView {
            gate: GateView {
                scope: &query,
                control: ControlState::initial(clock(0)?),
                outcomes: &outcomes,
            },
            group,
        },
        &retries,
        100,
        limits(),
        WORKSPACE,
    )?;
    let plan = decide_fresh_record_batch(fresh, group, &retries, 100)?;
    drop(encoded);
    assert_eq!(plan, legacy);
    let RecordBatchApplyDecision::Fresh {
        operation: wf_domain::workflow::indexed::IndexedDecision::Accepted(batch),
        outcome,
    } = plan.decision()
    else {
        return Err("creation not accepted".into());
    };
    assert_eq!(
        batch.batch().items()[0]
            .plan()
            .write()
            .ok_or("write absent")?
            .record
            .payload(),
        Some(b"v".as_slice())
    );
    assert_eq!(outcome.response(), [0]);
    assert_eq!(outcome.original_position(), log(0));
    assert_eq!(outcome.original_time(), TimestampMillis::new(1));
    assert_eq!(outcome.expires_at(), TimestampMillis::new(101));
    Ok(())
}
