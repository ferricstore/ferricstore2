//! Independent complete fault cohorts; no product state-publisher code.

use super::{TestResult, fixtures, seed_frame, target};
use std::collections::BTreeMap;
use wf_domain::claim::{ClaimRecord, DueOrder, LaneKey, LaneOrder};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp, plan_control};
use wf_domain::workflow::indexed::{IndexRejection, OrderingMetadata};
use wf_domain::workflow::request::{RetainedMetadata, RetainedOutcome, RetainedStatus};
use wf_domain::workflow::{CommittedCommand, DomainError, TransitionPlan, WorkflowRecord, reduce};
use wf_ports::log::{PreparedBytes, decode_prepared};
use wf_storage::state::{key, value};
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::TimestampMillis;

fn seed_plan() -> TestResult<TransitionPlan> {
    Ok(reduce(
        None,
        &CommittedCommand {
            id: RecordId::new(b"i".to_vec())?,
            now: TimestampMillis::new(1_000),
            operation: fixtures::create_op(),
        },
    )?)
}

fn insert_effect(
    rows: &mut BTreeMap<Vec<u8>, Vec<u8>>,
    plan: &TransitionPlan,
    ordering: OrderingMetadata,
    head: bool,
) -> TestResult {
    let scope = fixtures::scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = &plan.write().ok_or("expected real write absent")?.record;
    let history = plan.history().ok_or("expected history absent")?;
    rows.insert(
        key::encode_record_key(&scope, &partition, record.id(), fixtures::MIB)?,
        value::encode_record_fields(
            &scope,
            &partition,
            record,
            ordering,
            AdmissionLimits::default(),
            fixtures::MIB,
        )?,
    );
    rows.insert(
        key::encode_history_key(&scope, &partition, record.id(), history, fixtures::MIB)?,
        value::encode_history(&scope, &partition, record.id(), history, fixtures::MIB)?,
    );
    let entry = ClaimRecord {
        record,
        partition_key: b"p",
        priority: ordering.priority,
        state_enter_seq: ordering.state_enter_seq,
    };
    if let Some(lane) = LaneKey::for_record(&entry) {
        let member =
            key::encode_fifo_key(&scope, lane, LaneOrder::for_record(&entry), fixtures::MIB)?;
        assert_eq!(member.len(), 45);
        rows.insert(member, Vec::new());
        if head {
            let due = DueOrder::for_record(&entry).ok_or("expected scheduled head absent")?;
            let due_key = key::encode_due_key(&scope, due, fixtures::MIB)?;
            assert_eq!(due_key.len(), 37);
            rows.insert(due_key, vec![1]);
        }
    }
    Ok(())
}

fn insert_outcome(
    rows: &mut BTreeMap<Vec<u8>, Vec<u8>>,
    frame: &PreparedBytes,
    request: &[u8],
    index: u64,
    now: u64,
    status: RetainedStatus,
    response: &[u8],
) -> TestResult {
    let scope = fixtures::scope()?;
    let request = RequestId::new(request.to_vec())?;
    let decoded = decode_prepared(frame.as_bytes(), fixtures::driver_limits().codec)?;
    let outcome = RetainedOutcome::new(
        RetainedMetadata {
            scope: scope.clone(),
            request: request.clone(),
            intent: decoded.request().intent,
            original_position: fixtures::log(index),
            original_time: TimestampMillis::new(now),
            expires_at: TimestampMillis::new(now + 1_000),
            status,
        },
        1,
        response.to_vec(),
        AdmissionLimits::default(),
    )?;
    rows.insert(
        key::encode_outcome_key(&scope, &request, fixtures::MIB)?,
        value::encode_outcome(&outcome, fixtures::MIB)?,
    );
    Ok(())
}

fn control_key() -> TestResult<Vec<u8>> {
    let scope = fixtures::scope()?;
    Ok(key::encode_control_key(
        scope.group(),
        scope.recovery(),
        fixtures::MIB,
    )?)
}

fn expected_effect(
    rows: &mut BTreeMap<Vec<u8>, Vec<u8>>,
    kind: u8,
    now: u64,
    current: &WorkflowRecord,
) -> TestResult {
    match kind {
        0 => {
            let plan = reduce(
                None,
                &CommittedCommand {
                    id: RecordId::new(b"j".to_vec())?,
                    now: TimestampMillis::new(now),
                    operation: fixtures::create_op(),
                },
            )?;
            insert_effect(
                rows,
                &plan,
                OrderingMetadata {
                    priority: 0,
                    state_enter_seq: 1_u128 << 32,
                },
                false,
            )?;
        }
        1 => {
            let scope = fixtures::scope()?;
            let entry = ClaimRecord {
                record: current,
                partition_key: b"p",
                priority: 0,
                state_enter_seq: 0,
            };
            rows.remove(&key::encode_fifo_key(
                &scope,
                LaneKey::for_record(&entry).ok_or("seed lane absent")?,
                LaneOrder::for_record(&entry),
                fixtures::MIB,
            )?);
            rows.remove(&key::encode_due_key(
                &scope,
                DueOrder::for_record(&entry).ok_or("seed due absent")?,
                fixtures::MIB,
            )?);
            let plan = reduce(
                Some(current),
                &CommittedCommand {
                    id: current.id().clone(),
                    now: TimestampMillis::new(now),
                    operation: fixtures::cancel_op(1),
                },
            )?;
            insert_effect(
                rows,
                &plan,
                OrderingMetadata {
                    priority: 0,
                    state_enter_seq: 0,
                },
                false,
            )?;
        }
        2 => {
            assert_eq!(
                reduce(
                    Some(current),
                    &CommittedCommand {
                        id: current.id().clone(),
                        now: TimestampMillis::new(now),
                        operation: fixtures::create_op(),
                    }
                )
                .err(),
                Some(DomainError::AlreadyExists)
            );
        }
        3 => (),
        _ => return Err("unknown expected operation".into()),
    }
    Ok(())
}

pub(super) fn expected(
    genesis: &BTreeMap<Vec<u8>, Vec<u8>>,
    kind: Option<u8>,
    processing: u64,
) -> TestResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    assert_eq!(genesis.len(), 3);
    let scope = fixtures::scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let mut rows = genesis.clone();
    let seed = seed_plan()?;
    insert_effect(
        &mut rows,
        &seed,
        OrderingMetadata {
            priority: 0,
            state_enter_seq: 0,
        },
        true,
    )?;
    let frame = seed_frame()?;
    insert_outcome(
        &mut rows,
        &frame,
        b"seed",
        0,
        1_000,
        RetainedStatus::Accepted,
        &[0],
    )?;
    let mut control = plan_control(
        ControlState::initial(fixtures::clock(0)?),
        EntryIdentity::new(fixtures::log(0), frame.digest()),
        EntryStamp::Stamped(fixtures::clock(1_000)?),
    )?
    .next();
    if let Some(kind) = kind {
        let (frame, request, now) = target(kind)?;
        let current = &seed.write().ok_or("seed write absent")?.record;
        expected_effect(&mut rows, kind, now, current)?;
        if kind != 3 {
            let status = if kind == 2 {
                RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
                    DomainError::AlreadyExists,
                )))
            } else {
                RetainedStatus::Accepted
            };
            let response: &[u8] = if kind == 2 { &[1, 0, 1] } else { &[0] };
            insert_outcome(&mut rows, &frame, request, 1, now, status, response)?;
        }
        for index in 1..=processing {
            control = plan_control(
                control,
                EntryIdentity::new(fixtures::log(index), frame.digest()),
                EntryStamp::Stamped(fixtures::clock(now)?),
            )?
            .next();
        }
        assert_eq!(rows.len(), [12, 8, 9, 8][usize::from(kind)]);
    } else {
        assert_eq!(processing, 0);
        assert_eq!(rows.len(), 8);
    }
    rows.insert(
        control_key()?,
        value::encode_control(scope.group(), scope.recovery(), control, fixtures::MIB)?,
    );
    assert_eq!(rows[&control_key()?].len(), 110);
    let seed_key =
        key::encode_outcome_key(&scope, &RequestId::new(b"seed".to_vec())?, fixtures::MIB)?;
    assert_eq!(rows[&seed_key].len(), 110);
    assert!(rows.contains_key(&key::encode_record_key(
        &scope,
        &partition,
        &RecordId::new(b"i".to_vec())?,
        fixtures::MIB
    )?));
    Ok(rows)
}
