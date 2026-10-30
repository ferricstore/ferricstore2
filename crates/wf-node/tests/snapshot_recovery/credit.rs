//! Actual private credit bodies across startup audit, replay and snapshot custody.

use super::{TestResult, caps, f};
use wf_domain::governance::credit::CreditLimits;
use wf_domain::governance::identified::IdentifiedCreditDecision as D;
use wf_domain::workflow::request::RetainedStatus;
use wf_node::recovery::{
    CommittedRecoveryError as E, CommittedRecoveryLimits, purge_snapshot, recover_committed,
    recover_snapshotted, restore_received_snapshot,
};
use wf_ports::log::{CodecError, RaftLogStore, decode_credit_reply};
use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
use wf_ports::state::StateTransactionError;
use wf_storage::{
    snapshot::RedbSnapshotStage, state::RedbStateStore, transaction::CreditTransactionError,
};

mod audit;
mod fixtures;
use fixtures as c;

fn limits() -> TestResult<CommittedRecoveryLimits> {
    Ok(caps()?.with_identified_credit_limits(4, CreditLimits::default())?)
}

fn verify(state: &mut RedbStateStore) -> TestResult {
    let owner = c::owner(state)?;
    assert_eq!(
        (owner.view().limit, owner.view().free, owner.view().epoch),
        (3, 0, 2)
    );
    assert_eq!(
        (owner.view().cleanup_head, owner.view().cleanup_tail),
        (2, 1)
    );
    let lease = owner.lease(0).ok_or("missing shard lease")?;
    assert_eq!((lease.epoch, lease.available, lease.in_use), (2, 3, 0));
    assert_eq!(
        (lease.reservation_page, lease.reservation_page_fill),
        (0, 0)
    );
    let original = c::outcome(state, b"spend")?;
    assert_eq!(
        (original.intent().version(), original.response_version()),
        (12, 10)
    );
    assert_eq!(original.original_position(), f::at(1));
    assert_eq!(original.original_time().get(), 1001);
    assert_eq!(original.expires_at().get(), 2001);
    assert_eq!(original.status(), RetainedStatus::Accepted);
    let reply = decode_credit_reply(
        original.response(),
        CreditLimits::default(),
        16 * f::MIB,
        32 * f::MIB,
    )?;
    assert_eq!(reply.decision, D::Spent { shard_id: 0 });
    assert_eq!(reply.reservation_ids, c::ids());
    assert_eq!(
        reply
            .owner
            .ok_or("missing original response owner")?
            .lease(0)
            .ok_or("missing original response lease")?
            .in_use,
        2
    );
    let retry = c::outcome(state, b"retry")?;
    let retry = decode_credit_reply(
        retry.response(),
        CreditLimits::default(),
        16 * f::MIB,
        32 * f::MIB,
    )?;
    assert_eq!(retry.decision, D::Spent { shard_id: 0 });
    assert_eq!(retry.reservation_ids, c::ids());
    let lease = retry
        .owner
        .as_ref()
        .and_then(|o| o.lease(0))
        .ok_or("missing replay owner")?;
    assert_eq!(
        (lease.available, lease.in_use, lease.expires_at_ms),
        (2, 1, 2000)
    );
    Ok(())
}

#[test]
fn credit_node_replays_sparse_protocol_and_reaudits_historical_bodies() -> TestResult {
    let dir = f::directory("credit-node-replay")?;
    let (log, state, _) = c::input(&dir, false)?;
    let scope = f::scope()?;
    let (mut log, mut state) =
        recover_committed(log, state, scope.group(), scope.recovery(), limits()?)?;
    verify(&mut state)?;
    assert_eq!(log.state()?.committed(), Some(f::at(5)));
    assert_eq!(log.state()?.last_log(), Some(f::at(6)));
    drop((log, state));
    let before = c::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits()?,
    )?);
    assert_eq!(c::cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, logs);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_node_explicit_selection_capability_and_narrow_budgets_preserve_all_tables() -> TestResult
{
    let dir = f::directory("credit-node-refusal")?;
    let (log, state, _) = c::input(&dir, false)?;
    let scope = f::scope()?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits()?,
    )?);
    let before = c::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    for (selected, expected) in [
        (caps()?, E::UnsupportedEntry),
        (
            caps()?.with_identified_credit_limits(8, CreditLimits::default())?,
            E::Credit(CreditTransactionError::State(
                StateTransactionError::InvalidChanges,
            )),
        ),
        (
            caps()?
                .with_identified_credit_limits(4, CreditLimits::new(4, 1000, 32 * f::MIB, 1)?)?,
            E::Limit,
        ),
    ] {
        let (log, state) = f::open_pair(&dir)?;
        assert_eq!(
            recover_committed(log, state, scope.group(), scope.recovery(), selected).err(),
            Some(expected)
        );
        assert_eq!(c::cohort(&dir)?, before);
        assert_eq!(f::log_rows(&dir)?, logs);
    }
    let old = dir.join("old");
    std::fs::create_dir(&old)?;
    drop(c::pair(&old, None)?);
    let before = c::cohort(&old)?;
    let (log, state) = f::open_pair(&old)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), limits()?).err(),
        Some(E::Credit(CreditTransactionError::State(
            StateTransactionError::UnsupportedPolicyProfile
        )))
    );
    assert_eq!(c::cohort(&old)?, before);
    assert!(
        caps()?
            .with_identified_credit_limits(0, CreditLimits::default())
            .is_err()
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_node_snapshot_received_and_purged_suffix_keep_original_ids_and_outcomes() -> TestResult {
    let dir = f::directory("credit-node-snapshot")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let (log, mut state, frames) = c::input(&source, false)?;
    let mut wire = Vec::new();
    let snapshot = state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    assert_eq!(snapshot.control().position(), f::at(1).index());
    let scope = f::scope()?;
    let (log, state) = purge_snapshot(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits()?,
        SnapshotLimits::default(),
    )?;
    let (mut log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits()?,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.last_purged(), Some(f::at(1)));
    verify(&mut state)?;
    drop((log, state));
    let expected = f::state_rows(&source)?;
    let target = dir.join("target");
    std::fs::create_dir(&target)?;
    let mut log = c::log(&target)?;
    for (index, frame) in frames.iter().enumerate() {
        f::append(&mut log, u64::try_from(index)?, frame.as_bytes())?;
    }
    f::append(&mut log, 6, b"uncommitted opaque credit tail")?;
    let candidate = RedbSnapshotStage::receive(
        &target.join("state.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut std::io::Cursor::new(&wire),
        wf_storage::state::StateStoreLimits::default(),
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
    assert_eq!(log.state()?.committed(), Some(f::at(1)));
    log.save_committed(f::at(5))?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits()?,
        SnapshotLimits::default(),
    )?;
    verify(&mut state)?;
    drop((log, state));
    assert_eq!(f::state_rows(&target)?, expected);
    drop(f::open_pair(&target)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
