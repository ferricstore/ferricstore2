//! Exact pinned FIFO member and lane ordering assertions, without native storage.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::claim::{Claim, ClaimRecord, LaneKey, LaneOrder, decode_member, encode_member};
use wf_domain::lease::LeaseToken;
use wf_domain::workflow::{CommittedCommand, Create, Operation, WorkflowRecord, reduce};
use wf_types::identity::RecordId;
use wf_types::version::TimestampMillis;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn record(id: &[u8], state: &[u8]) -> Result<WorkflowRecord, Box<dyn std::error::Error>> {
    let command = CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(1_000),
        operation: Operation::Create(Create {
            workflow_type: b"orders".to_vec(),
            state: state.to_vec(),
            run_at: Some(TimestampMillis::new(2_000)),
            payload: None,
        }),
    };
    Ok(reduce(None, &command)?
        .into_parts()
        .1
        .ok_or("create write absent")?
        .record)
}

fn claimed(record: &WorkflowRecord) -> Result<WorkflowRecord, Box<dyn std::error::Error>> {
    let command = CommittedCommand {
        id: record.id().clone(),
        now: TimestampMillis::new(2_000),
        operation: Operation::Claim(Claim {
            expected_state: record.state().to_vec(),
            worker: b"worker".to_vec(),
            token: LeaseToken::new(b"lease".to_vec())?,
            lease_ms: 50,
        }),
    };
    Ok(reduce(Some(record), &command)?
        .into_parts()
        .1
        .ok_or("claim write absent")?
        .record)
}

fn entry(record: &WorkflowRecord, sequence: u128) -> ClaimRecord<'_> {
    ClaimRecord {
        record,
        partition_key: b"tenant-1",
        priority: 0,
        state_enter_seq: sequence,
    }
}

#[test]
fn member_round_trips_exact_128_bit_sequence_and_id() -> TestResult {
    // Source 7b6561ea0f73c257: flow/fifo_lane_test.exs:8-13, assertion line12.
    let id = RecordId::new(b"flow-1".to_vec())?;
    let member = encode_member(u128::MAX, &id);
    assert_eq!(
        decode_member(&member),
        Some((u128::MAX, b"flow-1".as_slice()))
    );
    Ok(())
}

#[test]
fn member_byte_order_preserves_sequence_beyond_float_precision() -> TestResult {
    // Source 4808d163b1ee3c1f: flow/fifo_lane_test.exs:15-20, assertion line19.
    let lower = encode_member(9_007_199_254_740_992, &RecordId::new(b"z".to_vec())?);
    let higher = encode_member(9_007_199_254_740_993, &RecordId::new(b"a".to_vec())?);
    assert!(lower < higher);
    Ok(())
}

#[test]
fn lane_rank_preserves_sequence_beyond_float_precision() -> TestResult {
    // Source 83dcf64f5941ec87: flow/fifo_lane_test.exs:22-36, assertions34-35.
    // BTreeMap rank replaces native index rank; no native backend score is claimed.
    let lower_record = record(b"z", b"queued")?;
    let higher_record = record(b"a", b"queued")?;
    let lower = entry(&lower_record, 9_007_199_254_740_992);
    let higher = entry(&higher_record, 9_007_199_254_740_993);
    let mut lane = BTreeMap::new();
    lane.insert(LaneOrder::for_record(&higher), higher);
    lane.insert(LaneOrder::for_record(&lower), lower);
    let (order, first) = lane.first_key_value().ok_or("lane is empty")?;
    assert_eq!(first.record.id().as_bytes(), b"z");
    assert_eq!(order.score(), 0);
    Ok(())
}

#[test]
fn decoder_rejects_truncation_and_empty_id() {
    // Source 6cb33b54203c08af: flow/fifo_lane_test.exs:38-41, assertions39-40.
    assert_eq!(decode_member(&[]), None);
    assert_eq!(decode_member(&1_u128.to_be_bytes()), None);
}

#[test]
fn running_record_remains_in_logical_state_lane() -> TestResult {
    // Source f72492d5892f240b: flow/fifo_lane_test.exs:43-56, assertions53-55.
    // Semantic lane identity replaces physical encoded key; colocation is omitted.
    let queued = record(b"flow-1", b"review")?;
    let running = claimed(&queued)?;
    let snapshot = entry(&running, 42);
    let lane = LaneKey::for_record(&snapshot).ok_or("running lane absent")?;
    assert_eq!(lane.workflow_type, b"orders");
    assert_eq!(lane.state, b"review");
    assert_eq!(lane.partition_key, b"tenant-1");
    let member = encode_member(snapshot.state_enter_seq, snapshot.record.id());
    assert_eq!(decode_member(&member), Some((42, b"flow-1".as_slice())));
    Ok(())
}

#[test]
fn running_members_rank_before_queued_even_with_later_sequence() -> TestResult {
    // Source 49ba97e84872f9d0: flow/fifo_lane_test.exs:58-89, assertions79-88.
    let queued_record = record(b"queued", b"review")?;
    let running_record = claimed(&record(b"running", b"review")?)?;
    let queued = entry(&queued_record, 1);
    let running = entry(&running_record, 2);
    let mut lane = BTreeMap::new();
    lane.insert(LaneOrder::for_record(&queued), queued);
    lane.insert(LaneOrder::for_record(&running), running);
    let rows: Vec<_> = lane.iter().collect();
    assert_eq!(rows[0].1.record.id().as_bytes(), b"running");
    assert_eq!(rows[1].1.record.id().as_bytes(), b"queued");
    assert_eq!(rows[0].0.score(), -1);
    assert_eq!(rows[1].0.score(), 0);
    Ok(())
}

#[test]
fn terminal_record_has_no_active_lane_identity() -> TestResult {
    // Source 835390d5953260a3: flow/fifo_lane_test.exs:124-133, assertion125.
    let terminal = record(b"flow-1", b"completed")?;
    assert_eq!(LaneKey::for_record(&entry(&terminal, 42)), None);
    Ok(())
}

#[test]
fn running_class_preserves_u128_sequence_then_binary_id_order() -> TestResult {
    // Supplemental total-order coverage; not a replacement for source assertions.
    let first_record = claimed(&record(&[255, 0], b"review")?)?;
    let second_record = claimed(&record(b"a", b"review")?)?;
    let first = entry(&first_record, u128::MAX - 1);
    let second = entry(&second_record, u128::MAX);
    assert!(LaneOrder::for_record(&first) < LaneOrder::for_record(&second));
    let tied = entry(&first_record, u128::MAX);
    assert!(LaneOrder::for_record(&second) < LaneOrder::for_record(&tied));
    Ok(())
}
