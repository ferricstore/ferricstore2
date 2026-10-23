//! Real atomic commits, original replies and independently bounded stored owners.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use sha2::{Digest, Sha256};
use wf_domain::claim::DueOrder;
use wf_domain::history::{EventKind, HistoryEntry};
use wf_domain::lease::LeaseGuard;
use wf_domain::policy::complete::{
    CompletePolicyData, CompleteStatePolicyData, PolicySchemaCharge, PolicySchemaLimits,
    prepare_complete_policy_value_v2, restore_complete_policy,
};
use wf_domain::policy::definition::StateMode;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::EntryIdentity;
use wf_domain::workflow::indexed::IndexRejection;
use wf_domain::workflow::request::{ClientIntent, RetainedOutcome, RetainedStatus};
use wf_domain::workflow::{CommittedCommand, Operation, StartAndClaim, Terminal};
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use wf_ports::log::{PreparedBytes, encode_atomic_start};
use wf_ports::state::StoredWorkflowRecord;
use wf_storage::state::value::StateValueError;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits, key, value};
use wf_storage::transaction::{
    RecordBatchTransactionOutcome, apply_atomic_start_transaction, apply_record_batch_transaction,
};
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::{Fence, TimestampMillis};

use super::{TestResult, fixtures as f};

const STORED_ATOMIC: &str = concat!(
    "4657534f000100000001740000000167000000017a0000000171000799eb58f7389d4e334a19b606",
    "718540304a002ef13e16290292f21baecba6ea9c0000000000000001000000000000000700000000",
    "00000001000000000000001100000000000003f90004000000724657415300010000000001700000",
    "00016900000001720000000171000000037700ff000000087700ff3a31373a310000000000000001",
    "00000000000000010000000000000012000000000000001100000000000000110000000000000001",
    "0000000000000000000000000100000000000000"
);

fn atomic_entry(request: &[u8], now: u64, payload: Option<&[u8]>) -> TestResult<PreparedBytes> {
    Ok(encode_atomic_start(
        PreparedRequest {
            scope: f::scope()?,
            request: RequestId::new(request.to_vec())?,
            intent: ClientIntent::new(1, [0; 32])?,
            stamp: f::clock(now)?,
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
        f::driver_limits().codec,
        f::driver_limits().workspace_bytes,
    )?)
}

fn atomic_store(path: &Path, fifo: bool) -> TestResult<RedbStateStore> {
    let scope = f::scope()?;
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
    let policies = [(scope.tenant().clone(), policy)];
    let placements = f::placements()?;
    let mut store = RedbStateStore::create(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0)?,
            placements: &placements,
            policies: &policies,
        },
        StateStoreLimits::default(),
    )?;
    let bootstrap = f::frame(b"bootstrap", 1, vec![(b"bootstrap", f::create_op())])?;
    let result = apply_record_batch_transaction(
        &mut store,
        &scope,
        bootstrap.as_bytes(),
        f::log(0),
        f::driver_limits(),
    )?;
    if !matches!(
        result,
        RecordBatchTransactionOutcome::Retained { outcome, .. }
            if outcome.status() == RetainedStatus::Accepted
    ) {
        return Err("real unrelated bootstrap did not commit".into());
    }
    Ok(store)
}

fn check_atomic_postimage(stored: &StoredWorkflowRecord, history: &HistoryEntry) -> TestResult {
    let record = &stored.record;
    let lease = record.lease().ok_or("persisted lease absent")?;
    if stored.tenant.as_bytes() != b"t"
        || stored.partition.as_bytes() != b"p"
        || record.id().as_bytes() != b"i"
        || record.workflow_type() != b"r"
        || record.state() != b"running"
        || record.run_state() != Some(b"q".as_slice())
        || record.version() != 1
        || record.fence() != Fence::new(1)
        || record.attempts() != 1
        || record.created_at() != TimestampMillis::new(17)
        || record.updated_at() != TimestampMillis::new(17)
        || record.next_run_at() != Some(TimestampMillis::new(18))
        || lease.worker.as_slice() != b"w\0\xff"
        || lease.token.as_bytes() != b"w\0\xff:17:1"
        || lease.deadline != TimestampMillis::new(18)
        || record.payload().is_some()
        || record.result().is_some()
        || record.error().is_some()
        || record.latest_event_witness() != (EventKind::Created, 1, Fence::new(1))
        || stored.ordering.priority != 0
        || stored.ordering.state_enter_seq != 1_u128 << 32
    {
        return Err("persisted leased atomic postimage differs".into());
    }
    if history.event_id != "17-1"
        || history.kind != EventKind::Created
        || history.occurred_at != TimestampMillis::new(17)
        || history.version != 1
        || history.fence != Fence::new(1)
        || history.attempts != 1
        || history.state.as_slice() != b"running"
        || history.next_run_at != Some(TimestampMillis::new(18))
        || history.lease_deadline != TimestampMillis::new(18)
        || history.worker.as_deref() != Some(b"w\0\xff".as_slice())
        || history.retry.is_some()
    {
        return Err("persisted Created history differs".into());
    }
    Ok(())
}

fn check_atomic_rows(
    path: &Path,
    before: &BTreeMap<Vec<u8>, Vec<u8>>,
    original: &RetainedOutcome,
) -> TestResult {
    let scope = f::scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let rows = f::raw_rows(path)?;
    let record_key = key::encode_record_key(&scope, &partition, &id, f::MIB)?;
    let outcome_key = key::encode_outcome_key(&scope, original.request(), f::MIB)?;
    let control_key = key::encode_control_key(scope.group(), scope.recovery(), f::MIB)?;
    let stored = value::decode_record(
        rows.get(&record_key).ok_or("atomic record absent")?,
        scope.group(),
        scope.recovery(),
        AdmissionLimits::default(),
        f::MIB,
    )?;
    let histories = rows
        .iter()
        .filter(|(key, _)| key.get(6) == Some(&5) && !before.contains_key(*key))
        .collect::<Vec<_>>();
    if histories.len() != 1 {
        return Err("atomic commit did not add exactly one history".into());
    }
    let history = value::decode_history(histories[0].1, &scope, &partition, &id, f::MIB)?;
    check_atomic_postimage(&stored, &history)?;
    let history_key = key::encode_history_key(&scope, &partition, &id, &history, f::MIB)?;
    let due = DueOrder {
        priority: 0,
        due: TimestampMillis::new(18),
        id: b"i",
        partition_key: b"p",
        state: b"q",
        workflow_type: b"r",
    };
    let due_key = key::encode_due_key(&scope, due, f::MIB)?;
    if rows.get(&due_key).map(Vec::as_slice) != Some(b"\0".as_slice())
        || key::decode_due_key(&due_key, &scope, f::MIB, f::MIB)?.due() != Some(due)
    {
        return Err("canonical parallel due row differs".into());
    }
    let added = rows
        .keys()
        .filter(|key| !before.contains_key(*key))
        .cloned()
        .collect::<BTreeSet<_>>();
    let expected = BTreeSet::from([record_key, history_key, due_key, outcome_key.clone()]);
    if added != expected || rows.len() != before.len() + 4 {
        return Err("unexpected atomic row set".into());
    }
    for (key, value) in before {
        if key != &control_key && rows.get(key) != Some(value) {
            return Err("unaffected predecessor row changed".into());
        }
    }
    let control = value::decode_control(
        rows.get(&control_key).ok_or("control absent")?,
        scope.group(),
        scope.recovery(),
        f::MIB,
    )?;
    let retained = value::decode_outcome(
        rows.get(&outcome_key).ok_or("outcome absent")?,
        &scope,
        original.request(),
        AdmissionLimits::default(),
        f::MIB,
    )?;
    if control.last_entry().map(EntryIdentity::log) != Some(f::log(1))
        || control.time() != f::clock(17)?
        || retained != *original
    {
        return Err("persisted original/control differs".into());
    }
    Ok(())
}

fn atomic_complete(path: &Path) -> TestResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    let rows = f::raw_rows(path)?;
    let scope = f::scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let point = key::encode_record_key(&scope, &partition, &id, f::MIB)?;
    let owner = value::decode_record(
        rows.get(&point).ok_or("record absent")?,
        scope.group(),
        scope.recovery(),
        AdmissionLimits::default(),
        f::MIB,
    )?;
    let lease = owner.record.lease().ok_or("actual lease absent")?;
    let complete = f::frame(
        b"complete",
        17,
        vec![(
            b"i",
            Operation::Complete(Terminal {
                guard: LeaseGuard {
                    token: lease.token.clone(),
                    fence: owner.record.fence(),
                },
                payload: None,
                value: Some(b"done".to_vec()),
            }),
        )],
    )?;
    let mut store = f::open(path)?;
    let result = apply_record_batch_transaction(
        &mut store,
        &scope,
        complete.as_bytes(),
        f::log(2),
        f::driver_limits(),
    )?;
    let RecordBatchTransactionOutcome::Retained { control, outcome } = result else {
        return Err("actual Complete did not retain its committed result".into());
    };
    if outcome.status() != RetainedStatus::Accepted
        || control.last_entry().map(EntryIdentity::log) != Some(f::log(2))
        || control.time() != f::clock(17)?
    {
        return Err("actual Complete control or status differs".into());
    }
    drop(store);
    f::raw_rows(path)
}

#[test]
fn atomic_start_commit_persists_leased_created_and_reopens() -> TestResult {
    let root = f::directory("atomic-start-created")?;
    let path = root.join("state.redb");
    drop(atomic_store(&path, false)?);
    let before = f::raw_rows(&path)?;
    let mut store = f::open(&path)?;
    let encoded = atomic_entry(b"q", 17, None)?;
    let result = apply_atomic_start_transaction(
        &mut store,
        &f::scope()?,
        encoded.as_bytes(),
        f::log(1),
        f::driver_limits(),
    );
    assert!(result.is_ok(), "{:?}", result.as_ref().err());
    let RecordBatchTransactionOutcome::Retained { control, outcome } = result? else {
        return Err("atomic commit did not return original reply".into());
    };
    assert_eq!(
        control.last_entry().ok_or("control absent")?.log(),
        f::log(1)
    );
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.response_version(), 4);
    assert_eq!(outcome.scope(), &f::scope()?);
    assert_eq!(outcome.request().as_bytes(), b"q");
    assert_eq!(outcome.intent().version(), 7);
    assert_eq!(outcome.response().len(), 114);
    assert_eq!(&outcome.response()[38..46], b"w\0\xff:17:1");
    assert_eq!(outcome.original_position(), f::log(1));
    assert_eq!(outcome.original_time(), TimestampMillis::new(17));
    assert_eq!(outcome.expires_at(), TimestampMillis::new(1_017));
    drop(store);
    check_atomic_rows(&path, &before, &outcome)?;
    let committed = f::raw_rows(&path)?;
    drop(f::open(&path)?);
    assert_eq!(f::raw_rows(&path)?, committed);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn atomic_start_persisted_fifo_refusal_writes_only_outcome_control() -> TestResult {
    let root = f::directory("atomic-start-fifo")?;
    let path = root.join("state.redb");
    drop(atomic_store(&path, true)?);
    let before = f::raw_rows(&path)?;
    let mut store = f::open(&path)?;
    let encoded = atomic_entry(b"q", 17, None)?;
    let result = apply_atomic_start_transaction(
        &mut store,
        &f::scope()?,
        encoded.as_bytes(),
        f::log(1),
        f::driver_limits(),
    );
    assert!(result.is_ok(), "{:?}", result.as_ref().err());
    let RecordBatchTransactionOutcome::Retained { control, outcome } = result? else {
        return Err("FIFO refusal did not retain its result".into());
    };
    assert_eq!(
        outcome.status(),
        RetainedStatus::IndexedRejected(IndexRejection::DirectRunningFifoEntry)
    );
    assert_eq!(outcome.response(), b"FWAS\0\x01\x01\x07\0");
    assert_eq!(outcome.original_position(), f::log(1));
    assert_eq!(outcome.original_time(), TimestampMillis::new(17));
    assert_eq!(outcome.expires_at(), TimestampMillis::new(1_017));
    drop(store);
    let rows = f::raw_rows(&path)?;
    let scope = f::scope()?;
    let control_key = key::encode_control_key(scope.group(), scope.recovery(), f::MIB)?;
    let outcome_key = key::encode_outcome_key(&scope, outcome.request(), f::MIB)?;
    assert_eq!(rows.len(), before.len() + 1);
    assert_eq!(
        rows.get(&outcome_key),
        Some(&value::encode_outcome(&outcome, f::MIB)?)
    );
    assert_eq!(
        rows.get(&control_key),
        Some(&value::encode_control(
            scope.group(),
            scope.recovery(),
            control,
            f::MIB,
        )?),
    );
    for (key, value) in &before {
        if key != &control_key {
            assert_eq!(rows.get(key), Some(value));
        }
    }
    drop(f::open(&path)?);
    assert_eq!(f::raw_rows(&path)?, rows);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn atomic_start_original_reply_survives_completion_and_lost_delivery() -> TestResult {
    let root = f::directory("atomic-start-original")?;
    let path = root.join("state.redb");
    drop(atomic_store(&path, false)?);
    let before = f::raw_rows(&path)?;
    let mut store = f::open(&path)?;
    let frame = atomic_entry(b"q", 17, None)?;
    let result = apply_atomic_start_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(1),
        f::driver_limits(),
    );
    assert!(result.is_ok(), "{:?}", result.as_ref().err());
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result? else {
        return Err("first reply absent".into());
    };
    let expected = outcome.as_ref().clone();
    drop(store);
    check_atomic_rows(&path, &before, &expected)?;
    drop(outcome); // Lose delivery only AFTER independent persisted-row checks.
    let completed = atomic_complete(&path)?;
    let scope = f::scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let point = key::encode_record_key(&scope, &partition, &id, f::MIB)?;
    let owner = value::decode_record(
        completed.get(&point).ok_or("completed record absent")?,
        scope.group(),
        scope.recovery(),
        AdmissionLimits::default(),
        f::MIB,
    )?;
    assert_eq!(owner.record.state(), b"completed");
    assert_eq!(owner.record.version(), 2);
    assert!(owner.record.lease().is_none());
    assert_eq!(owner.record.result(), Some(b"done".as_slice()));
    let histories = completed
        .iter()
        .filter(|(key, _)| key.get(6) == Some(&5) && !before.contains_key(*key))
        .map(|(_, wire)| value::decode_history(wire, &scope, &partition, &id, f::MIB))
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(histories.len(), 2);
    assert!(
        histories
            .iter()
            .any(|event| event.kind == EventKind::Created && event.event_id == "17-1")
    );
    assert!(
        histories
            .iter()
            .any(|event| event.kind == EventKind::Completed && event.event_id == "17-2")
    );
    let mut store = f::open(&path)?;
    let retry = atomic_entry(b"q", 18, None)?;
    let result = apply_atomic_start_transaction(
        &mut store,
        &scope,
        retry.as_bytes(),
        f::log(3),
        f::driver_limits(),
    )?;
    let RecordBatchTransactionOutcome::Retained { control, outcome } = result else {
        return Err("original reply was not replayed".into());
    };
    assert_eq!(*outcome, expected);
    assert_eq!(
        control.last_entry().ok_or("control absent")?.log(),
        f::log(3)
    );
    assert_eq!(control.time(), f::clock(18)?);
    drop(store);
    let replayed = f::raw_rows(&path)?;
    let control_key = key::encode_control_key(scope.group(), scope.recovery(), f::MIB)?;
    assert_eq!(replayed.len(), completed.len());
    for (key, value) in &completed {
        if key != &control_key {
            assert_eq!(replayed.get(key), Some(value));
        }
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn atomic_start_stored_outcome_admits_exact_original_owners() -> TestResult {
    let root = f::directory("atomic-start-owner")?;
    let path = root.join("state.redb");
    let mut store = atomic_store(&path, false)?;
    let frame = atomic_entry(b"q", 17, None)?;
    let result = apply_atomic_start_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(1),
        f::driver_limits(),
    );
    assert!(result.is_ok(), "{:?}", result.as_ref().err());
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result? else {
        return Err("actual donor outcome absent".into());
    };
    let golden = STORED_ATOMIC
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect::<TestResult<Vec<_>>>()?;
    assert_eq!(golden.len(), 220);
    assert_eq!(
        Sha256::digest(&golden).as_slice(),
        &[
            0x90, 0x58, 0x0c, 0x2d, 0xc4, 0x85, 0xdb, 0x21, 0x06, 0x59, 0x99, 0xeb, 0x3d, 0x91,
            0xe2, 0xb2, 0x88, 0x90, 0x5d, 0x05, 0x5a, 0x6f, 0x4a, 0x2e, 0x38, 0xe5, 0xf9, 0xde,
            0xf0, 0x69, 0x9c, 0x5c,
        ],
    );
    let wire = value::encode_outcome(&outcome, 220)?;
    assert_eq!(wire, golden);
    assert_eq!(
        value::encode_outcome(&outcome, 219).err(),
        Some(StateValueError::Limit {
            bytes: 220,
            maximum: 219
        })
    );
    let scope = f::scope()?;
    let request = RequestId::new(b"q".to_vec())?;
    let outcome_key = key::encode_outcome_key(&scope, &request, f::MIB)?;
    assert_eq!(outcome_key.len(), 19);
    assert_eq!(
        value::scan_outcome(&wire, &scope, &request, AdmissionLimits::default(), 219).err(),
        Some(StateValueError::Limit {
            bytes: 220,
            maximum: 219
        }),
    );
    let scanned = value::scan_outcome(&wire, &scope, &request, AdmissionLimits::default(), 220)?;
    assert_eq!(scanned.owned_bytes(), 630);
    assert_eq!(scanned.response_count(), Some(1));
    assert_eq!(
        scanned.into_owned(629).err(),
        Some(StateValueError::Limit {
            bytes: 630,
            maximum: 629
        })
    );
    for (inline, decision, valid) in [(8, 114, true), (8, 113, false), (7, 4_096, false)] {
        let profile = AdmissionLimits::new(inline, decision, 1)?;
        let result = value::scan_outcome(&wire, &scope, &request, profile, 220);
        if valid {
            assert_eq!(result?.owned_bytes(), 630);
        } else {
            assert_eq!(result.err(), Some(StateValueError::ProfileLimit));
        }
    }
    let owned = value::scan_outcome(&wire, &scope, &request, AdmissionLimits::default(), 220)?
        .into_owned(630)?;
    drop(outcome);
    drop(wire);
    drop(store);
    assert_eq!(f::raw_rows(&path)?.get(&outcome_key), Some(&golden));
    assert_eq!(owned.response(), &golden[106..]);
    assert_eq!(owned.original_position(), f::log(1));
    assert_eq!(owned.original_time(), TimestampMillis::new(17));
    assert_eq!(owned.expires_at(), TimestampMillis::new(1_017));
    assert_eq!(owned.status(), RetainedStatus::Accepted);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(root)?;
    Ok(())
}
