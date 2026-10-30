//! Real profile2 genesis, stored commands and complete database observations.

use std::path::Path;

use wf_domain::workflow::{Create, Operation, StartAndClaim};
use wf_ports::blob::OriginalValueRefKind;
use wf_ports::consensus::{PreparedBatchItem, WorkflowValuePolicyWitness};
use wf_ports::log::{PreparedBytes, RaftLogStore, encode_workflow_stored_value_batch};
use wf_storage::log::RedbLogStore;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::{
    RecordBatchTransactionOutcome, apply_workflow_stored_value_batch_transaction,
};
use wf_types::identity::PartitionId;

use super::{
    Error, Limits, TestResult, disk_backend, f, fixtures as v, recover_committed,
    recover_snapshotted,
};

mod audit;
mod custody;
mod replay;

fn limits() -> TestResult<Limits> {
    Ok(v::limits()?.with_workflow_stored_value_limits(
        1_024,
        64 * f::MIB,
        64 * f::MIB,
        64 * f::MIB,
    )?)
}

pub(super) fn pair(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore)> {
    let scope = f::scope()?;
    let placements = [(PartitionId::new(b"p".to_vec())?, scope.group().clone())];
    let state = RedbStateStore::create_stored_value_database(
        redb::Builder::new().create(dir.join("state.redb"))?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0, 0, 0)?,
            placements: &placements,
            policies: &[],
        },
        v::defaults(7_000),
        StateStoreLimits::default(),
    )?;
    Ok((v::log(dir)?, state))
}

fn seed(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore, WorkflowValuePolicyWitness)> {
    let (mut log, mut state) = pair(dir)?;
    let policy = v::install(&mut log, &mut state, true, None, 0, 10)?;
    Ok((log, state, policy))
}

fn item(mode: u8, id: &[u8], now: u64, payload: Option<&[u8]>) -> TestResult<PreparedBatchItem> {
    let operation = if mode == 7 {
        Operation::StartAndClaim(StartAndClaim {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            worker: b"worker".to_vec(),
            lease_ms: 5_000,
            payload: payload.map(<[u8]>::to_vec),
        })
    } else {
        Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: payload.map(<[u8]>::to_vec),
        })
    };
    v::item(id, now, operation)
}

fn encode(
    mode: u8,
    request: &[u8],
    now: u64,
    items: Vec<PreparedBatchItem>,
    policy: &WorkflowValuePolicyWitness,
) -> TestResult<PreparedBytes> {
    let batch = v::batch(
        &f::scope()?,
        mode,
        request,
        now,
        items,
        v::copy_policy(policy),
    )?;
    Ok(encode_workflow_stored_value_batch(
        &batch,
        v::limits()?.normal_limits().codec,
    )?)
}

fn apply(
    state: &mut RedbStateStore,
    frame: &PreparedBytes,
    at: u64,
) -> TestResult<RecordBatchTransactionOutcome> {
    Ok(apply_workflow_stored_value_batch_transaction(
        state,
        &f::scope()?,
        frame.as_bytes(),
        f::at(at),
        v::driver(&v::limits()?),
    )?)
}

fn pending(
    dir: &Path,
    mode: u8,
    payload: Option<&[u8]>,
    applied: bool,
) -> TestResult<(PreparedBytes, WorkflowValuePolicyWitness)> {
    let (mut log, mut state, policy) = seed(dir)?;
    let frame = encode(
        mode,
        b"entry",
        100,
        vec![item(mode, b"i", 100, payload)?],
        &policy,
    )?;
    f::append(&mut log, 1, frame.as_bytes())?;
    if applied {
        let result = apply(&mut state, &frame, 1)?;
        let RecordBatchTransactionOutcome::Retained { outcome, .. } = result else {
            return Err("direct stored producer did not retain outcome".into());
        };
        assert_eq!(outcome.intent().version(), 9);
    }
    log.save_committed(f::at(1))?;
    drop((log, state));
    Ok((frame, policy))
}

fn recover(dir: &Path, profile: &Limits) -> TestResult<(RedbLogStore, RedbStateStore)> {
    let scope = f::scope()?;
    let (log, state) = f::open_pair(dir)?;
    Ok(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        *profile,
    )?)
}

fn assert_payload(state: &RedbStateStore, id: &[u8], version: u64, expected: &[u8]) -> TestResult {
    let reference = v::reference(id, OriginalValueRefKind::Payload, version)?;
    let read = v::read(state, &[&reference, &reference])?;
    v::assert_read(&read, &[expected], &[Some(0), Some(0)]);
    Ok(())
}

fn refuse(dir: &Path, profile: &Limits, expected: Error) -> TestResult {
    let before = v::cohort(dir)?;
    let rows = f::log_rows(dir)?;
    let meta = f::log_meta(dir)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), *profile).err(),
        Some(expected)
    );
    drop(f::open_pair(dir)?);
    assert_eq!(v::cohort(dir)?, before);
    assert_eq!(f::log_rows(dir)?, rows);
    assert_eq!(f::log_meta(dir)?, meta);
    Ok(())
}
