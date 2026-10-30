//! Stored Step uses real named Claim, committed audit and received snapshot replay.

use super::{
    Error, Limits, TestResult, claim, f, fixtures as v, recover_committed, recover_snapshotted,
    stored,
};
use std::{io::Cursor, path::Path};
use wf_domain::claim::apply::ClaimWorkLimits;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::workflow::request::{ClientIntent, RetainedStatus};
use wf_domain::workflow::{Operation, StepContinue};
use wf_ports::consensus::{
    PreparedClaimPhase, PreparedClaimQuery, PreparedOperation, PreparedRequest,
};
use wf_ports::log::{
    PreparedBytes, RaftLogStore, encode_prepared, encode_stored_step_continue,
    encode_workflow_stored_value_batch,
};
use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction};
use wf_storage::log::RedbLogStore;
use wf_storage::snapshot::RedbSnapshotStage;
use wf_storage::state::{RedbStateStore, StateStoreLimits};
use wf_storage::transaction::apply_workflow_stored_value_batch_transaction;
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::Fence;

const PAYLOAD: &[u8] = b"FSV2\x02\x83t\0\0\0\x01w\x04stepm\0\0\0\x0bcharge_card";

fn limits() -> TestResult<Limits> {
    let selected =
        base()?.with_stored_step_continue_limits(1024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB);
    assert!(
        selected.is_ok(),
        "healthy stored Step recovery selection: {:?}",
        selected.as_ref().err()
    );
    Ok(selected?)
}

fn base() -> TestResult<Limits> {
    Ok(v::limits()?
        .with_workflow_stored_value_limits(1024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB)?
        .with_workflow_value_claim_limits(
            ClaimWorkLimits::default(),
            1024,
            64 * f::MIB,
            64 * f::MIB,
            64 * f::MIB,
        )?)
}

#[test]
fn stored_step_node_replays_full_typed_body_and_reaudits_historical_prefix() -> TestResult {
    let limits = limits()?;
    let dir = f::directory("stored-step-node")?;
    let (log, state, _) = input(&dir)?;
    let scope = f::scope()?;
    let (mut log, mut state) =
        recover_committed(log, state, scope.group(), scope.recovery(), limits)?;
    verify(&mut state)?;
    assert_eq!(log.state()?.committed(), Some(f::at(3)));
    assert_eq!(log.state()?.last_log(), Some(f::at(4)));
    let original = v::outcome(&mut state, b"step")?;
    assert_eq!(
        (original.intent().version(), original.response_version()),
        (11, 9)
    );
    assert_eq!(original.original_position(), f::at(3));
    assert_eq!(original.original_time().get(), 201);
    drop((log, state));
    let cohort = claim::cohort(&dir)?;
    let rows = f::log_rows(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits,
    )?);
    assert_eq!(claim::cohort(&dir)?, cohort);
    assert_eq!(f::log_rows(&dir)?, rows);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn stored_step_node_old_opt_ins_and_narrow_caps_preserve_whole_cohort() -> TestResult {
    let healthy = limits()?;
    let dir = f::directory("stored-step-node-caps")?;
    let (log, state, _) = input(&dir)?;
    drop((log, state));
    let cohort = claim::cohort(&dir)?;
    let rows = f::log_rows(&dir)?;
    let scope = f::scope()?;
    let expected = claim_prefix(&dir.join("control"))?;
    let old = base()?.with_step_continue_limits(1024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB)?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), old).err(),
        Some(Error::Codec(wf_ports::log::CodecError::UnsupportedVersion))
    );
    let applied_prefix = claim::cohort(&dir)?;
    assert_ne!(applied_prefix, cohort);
    assert_eq!(applied_prefix, expected);
    assert_eq!(f::log_rows(&dir)?, rows);
    for caps in [
        [0, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB],
        [1024, 0, 64 * f::MIB, 64 * f::MIB],
        [1024, 64 * f::MIB, 0, 64 * f::MIB],
        [1024, 64 * f::MIB, 64 * f::MIB, 0],
    ] {
        assert_eq!(
            base()?
                .with_stored_step_continue_limits(caps[0], caps[1], caps[2], caps[3])
                .err(),
            Some(Error::InvalidLimits)
        );
    }
    let narrow = base()?.with_stored_step_continue_limits(1024, 64 * f::MIB, 1, 64 * f::MIB)?;
    let (log, state) = f::open_pair(&dir)?;
    assert!(recover_committed(log, state, scope.group(), scope.recovery(), narrow).is_err());
    assert_eq!(claim::cohort(&dir)?, applied_prefix);
    assert_eq!(f::log_rows(&dir)?, rows);
    let (log, state) = f::open_pair(&dir)?;
    let (log, mut state) = recover_committed(log, state, scope.group(), scope.recovery(), healthy)?;
    verify(&mut state)?;
    drop((log, state));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn claim_prefix(dir: &Path) -> TestResult<[f::Rows; 6]> {
    std::fs::create_dir(dir)?;
    let (log, mut state, frames) = input(dir)?;
    let scope = f::scope()?;
    let outcome = wf_storage::transaction::apply_workflow_value_claim_transaction(
        &mut state,
        &scope,
        frames[2].as_bytes(),
        f::at(2),
        v::driver(&v::limits()?),
        ClaimWorkLimits::default(),
    )?;
    let wf_storage::transaction::ClaimTransactionOutcome::Retained { control, outcome } = outcome
    else {
        return Err("independent Claim prefix".into());
    };
    assert_eq!(control.position(), f::at(2).index());
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    drop((log, state));
    claim::cohort(dir)
}

#[test]
fn stored_step_node_snapshot_received_and_purged_suffix_preserve_typed_history() -> TestResult {
    use wf_node::recovery::{purge_snapshot, restore_received_snapshot};
    let limits = limits()?;
    let dir = f::directory("stored-step-node-snapshot")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let (log, mut state, frames) = input(&source)?;
    let mut wire = Vec::new();
    state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    let scope = f::scope()?;
    let (log, state) = purge_snapshot(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits,
        SnapshotLimits::default(),
    )?;
    let (mut log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.last_purged(), Some(f::at(1)));
    verify(&mut state)?;
    drop((log, state));
    let expected = claim::cohort(&source)?;
    let target = dir.join("target");
    std::fs::create_dir(&target)?;
    let mut log = v::log(&target)?;
    for (index, frame) in frames.iter().enumerate() {
        f::append(&mut log, u64::try_from(index)?, frame.as_bytes())?;
    }
    let candidate = RedbSnapshotStage::receive(
        &target.join("state.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let (mut log, state) = restore_received_snapshot(
        log,
        candidate,
        scope.group(),
        scope.recovery(),
        limits,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.committed(), Some(f::at(1)));
    log.save_committed(f::at(3))?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits,
        SnapshotLimits::default(),
    )?;
    verify(&mut state)?;
    drop((log, state));
    assert_eq!(claim::cohort(&target)?[1], expected[1]);
    drop(f::open_pair(&target)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn input(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore, Vec<PreparedBytes>)> {
    let (mut log, mut state) = stored::pair(dir)?;
    let policy = v::install(&mut log, &mut state, false, None, 0, 10)?;
    state = state.upgrade_history_snapshot_layout()?;
    let create = encode_workflow_stored_value_batch(
        &v::batch(
            &f::scope()?,
            5,
            b"create",
            100,
            vec![v::create(b"i", 100, b"old")?],
            v::copy_policy(&policy),
        )?,
        v::limits()?.normal_limits().codec,
    )?;
    f::append(&mut log, 1, create.as_bytes())?;
    let result = apply_workflow_stored_value_batch_transaction(
        &mut state,
        &f::scope()?,
        create.as_bytes(),
        f::at(1),
        v::driver(&v::limits()?),
    )?;
    let wf_storage::transaction::RecordBatchTransactionOutcome::Retained { control, outcome } =
        result
    else {
        return Err("stored Create acceptance".into());
    };
    assert_eq!(control.position(), f::at(1).index());
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.intent().version(), 9);
    assert_eq!(outcome.response_version(), 5);
    assert_eq!(outcome.original_position(), f::at(1));
    assert_eq!(outcome.original_time().get(), 100);
    assert_eq!(outcome.response(), [0]);
    let claim = encode_prepared(
        &PreparedRequest {
            scope: f::scope()?,
            request: RequestId::new(b"claim".to_vec())?,
            intent: ClientIntent::new(1, [42; 32])?,
            stamp: f::clock(200, 0, 200)?,
            operation: PreparedOperation::ClaimPhase(PreparedClaimPhase {
                query: PreparedClaimQuery {
                    workflow_type: b"r".to_vec(),
                    states: Some(vec![b"q".to_vec()]),
                    exclude_states: Vec::new(),
                    partitions: Some(vec![b"p".to_vec()]),
                    priority: None,
                    worker: b"worker".to_vec(),
                    lease_ms: 5000,
                    limit: 1,
                },
                token_version: 1,
                first_ordinal: u32::MAX,
                scan_budget: 1000,
                head_budget: 1000,
            }),
        },
        v::limits()?.normal_limits().codec,
    )?;
    let step = step_frame()?;
    f::append(&mut log, 2, claim.as_bytes())?;
    f::append(&mut log, 3, step.as_bytes())?;
    f::append(&mut log, 4, b"uncommitted opaque tail")?;
    log.save_committed(f::at(3))?;
    Ok((
        log,
        state,
        vec![v::policy_frame(false, None, 0, 10)?, create, claim, step],
    ))
}

fn step_frame() -> TestResult<PreparedBytes> {
    let encoded = encode_stored_step_continue(
        PreparedRequest {
            scope: f::scope()?,
            request: RequestId::new(b"step".to_vec())?,
            intent: ClientIntent::new(1, [0; 32])?,
            stamp: f::clock(201, 0, 201)?,
            operation: PreparedOperation::Batch(vec![v::item(
                b"i",
                201,
                Operation::StepContinue(StepContinue {
                    guard: LeaseGuard {
                        token: LeaseToken::new(b"worker:200:1".to_vec())?,
                        fence: Fence::new(1),
                    },
                    from: b"q".to_vec(),
                    to: b"charge_card".to_vec(),
                    worker: None,
                    lease_ms: 5000,
                    payload: Some(PAYLOAD.to_vec()),
                }),
            )?]),
        },
        v::limits()?.normal_limits().codec,
        512 * f::MIB,
    )?;
    Ok(encoded)
}

fn verify(state: &mut RedbStateStore) -> TestResult {
    let scope = f::scope()?;
    let stored = state
        .begin_apply(
            scope.group(),
            scope.recovery(),
            v::limits()?.normal_limits().codec.admission(),
            10000,
            64 * f::MIB,
            32 * f::MIB,
        )?
        .record(
            scope.tenant(),
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
        )?
        .ok_or("record")?;
    let record = &stored.record;
    assert_eq!(
        (record.version(), record.fence().get(), record.attempts()),
        (3, 2, 0)
    );
    assert_eq!(record.state(), b"running");
    assert_eq!(record.run_state(), Some(b"charge_card".as_slice()));
    assert_eq!(record.payload(), Some(PAYLOAD));
    assert_eq!(record.lease().ok_or("lease")?.deadline.get(), 5201);
    assert_eq!(
        v::outcome(state, b"step")?.status(),
        RetainedStatus::Accepted
    );
    assert_eq!(v::control(state)?.position(), f::at(3).index());
    Ok(())
}
