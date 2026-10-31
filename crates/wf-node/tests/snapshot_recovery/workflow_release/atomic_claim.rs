//! Actual original-parent startup and historical recovery, never leaf emulation.
use super::{E, TestResult, f, fixtures as p, frames as q};
use wf_domain::{
    claim::{
        caller::{
            ClaimCallerLimits, ClaimDueInput, ClaimPartitionsInput, ClaimStatesInput,
            normalize_claim_due,
        },
        coordination::ClaimCoordinationLimits,
    },
    governance::release::LimitEnforcement,
};
use wf_node::recovery::{CommittedRecoveryLimits, recover_committed};
use wf_ports::log::{
    AtomicClaimOptions, GovernedClaimPayment, PreparedBytes, encode_atomic_claim_request,
    inspect_atomic_claim_reply,
};

fn limits() -> TestResult<CommittedRecoveryLimits> {
    Ok(p::selected(true)?.with_atomic_claim_recovery(
        ClaimCallerLimits::default(),
        ClaimCoordinationLimits::default(),
    )?)
}

fn frames() -> TestResult<Vec<PreparedBytes>> {
    let mut frames = q::all()?;
    let keys: [&[u8]; 1] = [b"p"];
    let mut input = ClaimDueInput::new(b"r", b"w");
    input.states = ClaimStatesInput::One(b"q");
    input.partitions = ClaimPartitionsInput::Many(&keys);
    input.limit = Some(2);
    input.lease_ms = Some(30000);
    input.governance_limit_scope = Some(b"global");
    input.governance_shard_id = Some(0);
    let call = normalize_claim_due(input, ClaimCallerLimits::default())?;
    frames[4] = encode_atomic_claim_request(
        &f::scope()?,
        &wf_types::identity::RequestId::new(b"claim".to_vec())?,
        f::clock(104, 0, 104)?,
        &call,
        AtomicClaimOptions {
            token_version: 1,
            first_ordinal: 0,
            scan_budget: 1000,
            head_budget: 1000,
            payment: Some(GovernedClaimPayment {
                scope: b"global",
                shard_id: 0,
                epoch: 1,
                enforcement: LimitEnforcement::ApproximateGlobal,
                reservation_ids: &[b"a\0".to_vec(), b"b".to_vec()],
            }),
        },
        f::driver_limits().codec,
        128 * f::MIB,
    )?;
    Ok(frames)
}

fn check(state: &mut wf_storage::state::RedbStateStore) -> TestResult {
    use wf_ports::state::{GroupApplyStore, GroupApplyTransaction};
    let scope = f::scope()?;
    assert_eq!(p::record(state, b"i")?.record.state(), b"cancelled");
    assert_eq!(p::record(state, b"j")?.record.state(), b"running");
    let tx = state.begin_apply(
        scope.group(),
        scope.recovery(),
        f::driver_limits().codec.admission(),
        10000,
        64 * f::MIB,
        32 * f::MIB,
    )?;
    let outcome = tx
        .outcome(
            scope.tenant(),
            &wf_types::identity::RequestId::new(b"claim".to_vec())?,
        )?
        .ok_or("original parent receipt")?;
    assert_eq!(
        (
            outcome.intent().version(),
            outcome.response_version(),
            outcome.original_position()
        ),
        (20, 17, f::at(4))
    );
    let reply = inspect_atomic_claim_reply(
        outcome.response(),
        outcome.original_time(),
        f::driver_limits().codec.admission(),
    )?;
    assert_eq!(reply.count(), 2);
    assert_eq!(
        reply
            .used_reservation_ids()
            .collect::<Result<Vec<_>, _>>()?,
        [b"a\0".as_slice(), b"b".as_slice()]
    );
    assert_eq!(tx.control()?.position(), f::at(8).index());
    Ok(())
}

#[test]
fn original_parent_replays_the_real_paid_terminal_release_and_ack_timeline() -> TestResult {
    use wf_ports::log::RaftLogStore;
    let dir = f::directory("atomic-node-paid-parent")?;
    let (mut log, state) = p::pair(&dir, true)?;
    let frames = frames()?;
    p::append(&mut log, &frames, None)?;
    let scope = f::scope()?;
    let (mut log, mut state) =
        recover_committed(log, state, scope.group(), scope.recovery(), limits()?)?;
    check(&mut state)?;
    assert_eq!(log.state()?.last_log(), Some(f::at(9)));
    drop((log, state));
    let before = p::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits()?,
    )?);
    assert_eq!(p::cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, logs);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn preceding_paid_leaf_profile_refuses_original_parent_without_applying_it() -> TestResult {
    let dir = f::directory("atomic-node-old-paid-profile")?;
    let (mut log, mut state) = p::pair(&dir, true)?;
    let frames = frames()?;
    p::apply(&mut state, &frames, 4)?;
    p::append(&mut log, &frames, None)?;
    drop((log, state));
    let before = p::cohort(&dir)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(
            log,
            state,
            scope.group(),
            scope.recovery(),
            p::selected(true)?
        )
        .err(),
        Some(E::UnsupportedEntry)
    );
    assert_eq!(p::cohort(&dir)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn native_limits() -> TestResult<wf_storage::transaction::AtomicClaimTransactionLimits> {
    Ok(wf_storage::transaction::AtomicClaimTransactionLimits {
        values: p::values()?,
        work: wf_domain::claim::apply::ClaimWorkLimits::default(),
        caller: ClaimCallerLimits::default(),
        coordination: ClaimCoordinationLimits::default(),
        credit: wf_domain::governance::credit::CreditLimits::default(),
    })
}

#[test]
fn complete_historical_parent_is_audited_behind_a_valid_later_control_boundary() -> TestResult {
    use wf_domain::workflow::control::EntryStamp;
    use wf_ports::log::{CodecError, RaftLogStore};
    let dir = f::directory("atomic-node-historical-body")?;
    let (mut log, mut state) = p::pair(&dir, true)?;
    let frames = frames()?;
    for (index, frame) in frames.iter().take(5).enumerate() {
        let mut body = frame.as_bytes().to_vec();
        if index == 4 {
            body.push(0);
        }
        f::append(&mut log, u64::try_from(index)?, &body)?;
    }
    p::apply(&mut state, &frames, 4)?;
    wf_storage::transaction::apply_atomic_claim_transaction(
        &mut state,
        &f::scope()?,
        frames[4].as_bytes(),
        f::at(4),
        native_limits()?,
    )?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::append(&mut log, 5, blank.as_bytes())?;
    f::apply_control(&mut state, 5, blank.as_bytes())?;
    log.save_committed(f::at(5))?;
    drop((log, state));
    let before = p::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), limits()?).err(),
        Some(E::Codec(CodecError::TrailingBytes))
    );
    assert_eq!(p::cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, logs);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn parent_selection_requires_actual_dependencies_and_rechecks_later_builder_weakening() -> TestResult
{
    let caller = ClaimCallerLimits::default();
    let coordination = ClaimCoordinationLimits::default();
    assert!(matches!(
        super::caps()?.with_atomic_claim_recovery(caller, coordination),
        Err(E::InvalidLimits)
    ));
    assert!(matches!(
        p::selected(false)?.with_atomic_claim_recovery(caller, coordination),
        Err(E::InvalidLimits)
    ));
    let dir = f::directory("atomic-node-weakened-profile")?;
    drop(p::pair(&dir, true)?);
    let before = p::cohort(&dir)?;
    let scope = f::scope()?;
    let weakened = limits()?.with_governance_catalogs();
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), weakened).err(),
        Some(E::InvalidLimits)
    );
    assert_eq!(p::cohort(&dir)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn purged_and_received_snapshots_replay_original_parent_ids_and_preserve_the_uncommitted_suffix()
-> TestResult {
    use wf_node::recovery::{purge_snapshot, recover_snapshotted, restore_received_snapshot};
    use wf_ports::{
        log::RaftLogStore,
        snapshot::{SnapshotLimits, StateSnapshotStore},
    };
    use wf_storage::{snapshot::RedbSnapshotStage, state::StateStoreLimits};
    let dir = f::directory("atomic-node-parent-snapshots")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let frames = frames()?;
    let (mut log, mut state) = p::pair(&source, true)?;
    p::append(&mut log, &frames, None)?;
    p::apply(&mut state, &frames, 4)?;
    let mut prefix = Vec::new();
    state.capture_snapshot(&mut prefix, SnapshotLimits::default())?;
    let scope = f::scope()?;
    let (log, state) = purge_snapshot(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits()?,
        SnapshotLimits::default(),
    )?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits()?,
        SnapshotLimits::default(),
    )?;
    check(&mut state)?;
    drop((log, state));
    let expected = p::cohort(&source)?;
    let received = dir.join("received");
    std::fs::create_dir(&received)?;
    let mut log = p::log(&received)?;
    for (index, frame) in frames.iter().enumerate() {
        f::append(&mut log, u64::try_from(index)?, frame.as_bytes())?;
    }
    f::append(&mut log, 9, b"opaque uncommitted original-parent suffix")?;
    let candidate = RedbSnapshotStage::receive(
        &received.join("state.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut std::io::Cursor::new(prefix),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let (mut log, state) = restore_received_snapshot(
        log,
        candidate,
        scope.group(),
        scope.recovery(),
        limits()?,
        SnapshotLimits::default(),
    )?;
    log.save_committed(f::at(8))?;
    let (mut log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits()?,
        SnapshotLimits::default(),
    )?;
    check(&mut state)?;
    assert_eq!(log.state()?.last_log(), Some(f::at(9)));
    drop((log, state));
    assert_eq!(p::cohort(&received)?, expected);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn historical_parent_preserves_its_explicit_value_work_ceiling() -> TestResult {
    use wf_domain::workflow::control::EntryStamp;
    use wf_ports::log::RaftLogStore;
    let dir = f::directory("atomic-node-value-work-profile")?;
    let (mut log, mut state) = p::pair(&dir, true)?;
    let keys: [&[u8]; 1] = [b"p"];
    let mut input = ClaimDueInput::new(b"r", b"w");
    input.partitions = ClaimPartitionsInput::Many(&keys);
    let call = normalize_claim_due(input, ClaimCallerLimits::default())?;
    let scope = f::scope()?;
    let frame = encode_atomic_claim_request(
        &scope,
        &wf_types::identity::RequestId::new(b"empty".to_vec())?,
        f::clock(100, 0, 100)?,
        &call,
        AtomicClaimOptions {
            token_version: 1,
            first_ordinal: 0,
            scan_budget: 100,
            head_budget: 100,
            payment: None,
        },
        f::driver_limits().codec,
        128 * f::MIB,
    )?;
    let mut native = native_limits()?;
    native.values.maximum_value_work = 512 * 1024;
    wf_storage::transaction::apply_atomic_claim_transaction(
        &mut state,
        &scope,
        frame.as_bytes(),
        f::at(0),
        native,
    )?;
    f::append(&mut log, 0, frame.as_bytes())?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::apply_control(&mut state, 1, blank.as_bytes())?;
    f::append(&mut log, 1, blank.as_bytes())?;
    log.save_committed(f::at(1))?;
    drop((log, state));
    let before = p::cohort(&dir)?;
    let too_small = limits()?.with_workflow_value_claim_limits(
        wf_domain::claim::apply::ClaimWorkLimits::default(),
        1024,
        64 * f::MIB,
        32768,
        32 * f::MIB,
    )?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), too_small).err(),
        Some(E::Limit)
    );
    assert_eq!(p::cohort(&dir)?, before);
    let selected = limits()?.with_workflow_value_claim_limits(
        wf_domain::claim::apply::ClaimWorkLimits::default(),
        1024,
        64 * f::MIB,
        512 * 1024,
        32 * f::MIB,
    )?;
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        selected,
    )?);
    assert_eq!(p::cohort(&dir)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
