//! The dedicated driver must discover policy and consume commit before reply.

use std::collections::BTreeMap;

use wf_domain::history::EventKind;
use wf_domain::policy::complete::{
    CompletePolicyData, CompleteStatePolicyData, prepare_complete_policy_value_v2,
    restore_complete_policy,
};
use wf_domain::policy::definition::StateMode;
use wf_domain::workflow::StartAndClaim;
use wf_domain::workflow::indexed::IndexRejection;
use wf_ports::log::encode_atomic_start;
use wf_storage::transaction::apply_atomic_start_transaction;
use wf_types::version::Fence;

use super::*;

fn atomic_entry(request: &[u8], now: u64, payload: Option<&[u8]>) -> TestResult<Vec<u8>> {
    let encoded = encode_atomic_start(
        PreparedRequest {
            scope: scope()?,
            request: RequestId::new(request.to_vec())?,
            intent: ClientIntent::new(1, [0; 32])?,
            stamp: clock(now)?,
            operation: PreparedOperation::Batch(vec![PreparedBatchItem {
                partition: PartitionId::new(b"p".to_vec())?,
                command: CommittedCommand {
                    id: RecordId::new(b"i".to_vec())?,
                    now: TimestampMillis::new(now),
                    operation: Operation::StartAndClaim(StartAndClaim {
                        workflow_type: b"r".to_vec(),
                        state: b"q".to_vec(),
                        worker: b"w\0\xff".to_vec(),
                        lease_ms: 1,
                        payload: payload.map(<[u8]>::to_vec),
                    }),
                },
                create_priority: 0,
            }]),
        },
        limits().codec,
        limits().workspace_bytes,
    )?;
    Ok(encoded.into_bytes())
}

fn atomic_policy(fifo: bool) -> TestResult<Vec<u8>> {
    let schema = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(schema);
    let candidate = restore_complete_policy(
        1,
        CompletePolicyData {
            workflow_type: b"r".to_vec(),
            version: None,
            max_active_ms: None,
            retry: None,
            retention: None,
            states: BTreeMap::from([(
                b"q".to_vec(),
                CompleteStatePolicyData {
                    mode: Some(if fifo {
                        StateMode::Fifo
                    } else {
                        StateMode::Parallel
                    }),
                    retry: None,
                    retention: None,
                    governance: None,
                },
            )]),
            indexed_attributes: None,
            indexed_state_meta: None,
            governance: None,
        },
        schema,
        &mut charge,
    )?;
    let policy = prepare_complete_policy_value_v2(candidate, schema, &mut charge)?;
    Ok(policy.encoded().to_vec())
}

#[test]
fn atomic_start_driver_discovers_fresh_policy_and_consumes_commit() -> TestResult {
    for fifo in [false, true] {
        let mut store = observed_store()?;
        store.control = processed()?;
        store
            .policy_bytes
            .push((b"r".to_vec(), atomic_policy(fifo)?));
        store.parallel = !fifo;
        let encoded = atomic_entry(b"q", 17, None)?;
        let result =
            apply_atomic_start_transaction(&mut store, &scope()?, &encoded, log(1), limits());
        assert!(result.is_ok(), "{:?}", result.as_ref().err());
        let RecordBatchTransactionOutcome::Retained { control, outcome } = result? else {
            return Err("no committed retained atomic reply".into());
        };
        assert!(store.committed);
        assert_eq!(store.next_control, Some(control));
        assert_eq!(store.captured_admission, Some(limits().codec.admission()));
        assert_eq!(*store.queried_types.borrow(), [b"r".to_vec()]);
        assert_eq!(
            &store.calls.borrow()[..4],
            ["begin", "control", "retained", "outcome"]
        );
        assert_eq!(store.calls.borrow().last(), Some(&"commit"));
        assert_eq!(store.writes.len(), usize::from(!fifo));
        assert_eq!(store.histories.len(), usize::from(!fifo));
        assert_eq!(store.written_outcome.as_ref(), Some(outcome.as_ref()));
        let status = if fifo {
            RetainedStatus::IndexedRejected(IndexRejection::DirectRunningFifoEntry)
        } else {
            RetainedStatus::Accepted
        };
        assert_eq!(outcome.status(), status);
        assert_eq!(outcome.original_position(), log(1));
        assert_eq!(outcome.original_time(), TimestampMillis::new(17));
        assert_eq!(outcome.expires_at(), TimestampMillis::new(1_017));
        if fifo {
            assert_eq!(outcome.response(), hex("465741530001010700")?);
        } else {
            assert_atomic_record(&store)?;
        }
    }
    for failure in [
        StateTransactionError::KnownAborted,
        StateTransactionError::Uncertain,
    ] {
        let mut store = observed_store()?;
        store.control = processed()?;
        store.failure = Some(("commit", failure));
        let result = apply_atomic_start_transaction(
            &mut store,
            &scope()?,
            &atomic_entry(b"q", 17, None)?,
            log(1),
            limits(),
        );
        assert!(matches!(
            result,
            Err(RecordBatchTransactionError::Transaction(error)) if error == failure
        ));
        assert!(!store.committed);
        assert!(store.written_outcome.is_none());
        assert!(store.writes.is_empty() && store.histories.is_empty());
    }
    Ok(())
}

#[test]
fn atomic_start_driver_replays_original_before_current_reads() -> TestResult {
    let mut store = observed_store()?;
    store.control = processed()?;
    let first = atomic_entry(b"q", 17, None)?;
    let result = apply_atomic_start_transaction(&mut store, &scope()?, &first, log(1), limits());
    assert!(result.is_ok(), "{:?}", result.as_ref().err());
    let RecordBatchTransactionOutcome::Retained { control, outcome } = result? else {
        return Err("first atomic result was not retained".into());
    };
    assert!(store.committed);
    let address = outcome.response().as_ptr();
    let expected = outcome.as_ref().clone();
    store.control = control;
    store.retained_present = true;
    store.retained = RefCell::new(Some(*outcome));
    store.committed = false;
    store.next_control = None;
    store.written_outcome = None;
    store.writes.clear();
    store.histories.clear();
    store.calls.borrow_mut().clear();
    store.queried_types.borrow_mut().clear();
    store.failure = Some(("placement", StateTransactionError::Unavailable));
    let retry = atomic_entry(b"q", 18, None)?;
    let result = apply_atomic_start_transaction(&mut store, &scope()?, &retry, log(2), limits())?;
    let RecordBatchTransactionOutcome::Retained { control, outcome } = result else {
        return Err("original atomic replay absent".into());
    };
    assert!(store.committed);
    assert_eq!(
        *store.calls.borrow(),
        ["begin", "control", "retained", "outcome", "commit"]
    );
    assert!(store.retained.borrow().is_none());
    assert!(store.writes.is_empty() && store.histories.is_empty());
    assert!(store.written_outcome.is_none());
    assert!(store.queried_types.borrow().is_empty());
    assert_eq!(store.next_control, Some(control));
    assert_eq!(control.last_entry().ok_or("control absent")?.log(), log(2));
    assert_eq!(control.time(), clock(18)?);
    drop(store);
    drop(first);
    drop(retry);
    assert_eq!(*outcome, expected);
    assert_eq!(outcome.response().as_ptr(), address);
    assert_eq!(outcome.original_position(), log(1));
    assert_eq!(outcome.original_time(), TimestampMillis::new(17));
    assert_eq!(outcome.expires_at(), TimestampMillis::new(1_017));
    Ok(())
}

fn assert_atomic_record(store: &ObservedStore) -> TestResult {
    let record = &store.writes[0].record;
    assert_eq!(store.writes[0].expected_version, None);
    assert_eq!(
        (record.version(), record.fence(), record.attempts()),
        (1, Fence::new(1), 1)
    );
    assert_eq!(record.state(), b"running");
    assert_eq!(record.run_state(), Some(b"q".as_slice()));
    assert_eq!(record.next_run_at(), Some(TimestampMillis::new(18)));
    assert_eq!(
        record.lease().ok_or("lease absent")?.token.as_bytes(),
        b"w\0\xff:17:1"
    );
    assert_eq!(store.histories[0].kind, EventKind::Created);
    assert_eq!(store.histories[0].event_id, "17-1");
    Ok(())
}
