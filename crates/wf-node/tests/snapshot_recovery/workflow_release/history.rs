//! Full committed replay and complete historical bodies behind valid boundaries.
use super::*;
use super::{fixtures as p, frames as q};
use wf_node::recovery::recover_committed;
use wf_ports::log::{CodecError, RaftLogStore};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction};

#[test]
fn release_node_refuses_unpaid_combined_acquisition_in_fresh_and_historical_paths() -> TestResult {
    use wf_domain::workflow::control::EntryStamp;
    use wf_storage::transaction::{
        WorkflowValueTransactionError, apply_workflow_stored_value_batch_transaction,
    };
    let s = f::scope()?;
    let frame = q::combined()?;
    for historical in [false, true] {
        let dir = f::directory("release-node-unpaid-combined")?;
        let (mut log, mut state) = p::pair(&dir, true)?;
        f::append(&mut log, 0, frame.as_bytes())?;
        let expected = if historical {
            apply_workflow_stored_value_batch_transaction(
                &mut state,
                &s,
                frame.as_bytes(),
                f::at(0),
                p::values()?,
            )?;
            let blank = f::blank(EntryStamp::Unstamped)?;
            f::append(&mut log, 1, blank.as_bytes())?;
            f::apply_control(&mut state, 1, blank.as_bytes())?;
            log.save_committed(f::at(1))?;
            E::UnsupportedEntry
        } else {
            log.save_committed(f::at(0))?;
            E::Value(WorkflowValueTransactionError::UnsupportedProfile)
        };
        drop((log, state));
        let before = p::cohort(&dir)?;
        let logs = f::log_rows(&dir)?;
        let (log, state) = f::open_pair(&dir)?;
        assert_eq!(
            recover_committed(log, state, s.group(), s.recovery(), p::selected(true)?).err(),
            Some(expected)
        );
        assert_eq!(p::cohort(&dir)?, before);
        assert_eq!(f::log_rows(&dir)?, logs);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn release_node_gc4_routes_mutations_and_original_maintenance_in_its_own_profile() -> TestResult {
    use wf_domain::governance::release::{
        ReleaseMaintenanceRejection, ReleaseMaintenanceRequest as C,
    };
    use wf_domain::workflow::request::RetainedStatus;
    let dir = f::directory("release-node-gc4-body")?;
    let (mut log, state) = p::pair(&dir, false)?;
    let frames = vec![
        q::stored_frame(b"i", 100, f::create_op())?,
        q::maintenance(b"empty-mark", 101, &C::MarkCompleted { sequences: vec![1] })?,
    ];
    p::append(&mut log, &frames, None)?;
    let s = f::scope()?;
    let (log, mut state) =
        recover_committed(log, state, s.group(), s.recovery(), p::selected(false)?)?;
    assert_eq!(p::record(&mut state, b"i")?.record.state(), b"q");
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        f::driver_limits().codec.admission(),
        10000,
        64 * f::MIB,
        32 * f::MIB,
    )?;
    let receipt = tx
        .outcome(
            s.tenant(),
            &wf_types::identity::RequestId::new(b"empty-mark".to_vec())?,
        )?
        .ok_or("mark receipt")?;
    assert_eq!(
        (receipt.intent().version(), receipt.response_version()),
        (18, 15)
    );
    assert_eq!(
        receipt.status(),
        RetainedStatus::WorkflowReleaseRejected(ReleaseMaintenanceRejection::InvalidCompletion)
    );
    drop(tx);
    drop((log, state));
    let before = p::cohort(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        s.group(),
        s.recovery(),
        p::selected(false)?,
    )?);
    assert_eq!(p::cohort(&dir)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn release_node_replays_actual_paid_work_and_reaudits_original_bodies() -> TestResult {
    let dir = f::directory("release-node-paid-replay")?;
    let (mut log, state) = p::pair(&dir, true)?;
    let frames = q::all()?;
    p::append(&mut log, &frames, None)?;
    let s = f::scope()?;
    let (mut log, mut state) =
        recover_committed(log, state, s.group(), s.recovery(), p::selected(true)?)?;
    p::check(&mut state)?;
    assert_eq!(log.state()?.committed(), Some(f::at(8)));
    assert_eq!(log.state()?.last_log(), Some(f::at(9)));
    drop((log, state));
    let before = p::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        s.group(),
        s.recovery(),
        p::selected(true)?,
    )?);
    assert_eq!(p::cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, logs);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn release_node_audits_complete_paid_claim_and_maintenance_behind_valid_applied_boundary()
-> TestResult {
    let s = f::scope()?;
    let frames = q::all()?;
    for index in [4, 7] {
        let dir = f::directory("release-node-historical-trailing")?;
        let (mut log, mut state) = p::pair(&dir, true)?;
        p::append(&mut log, &frames, Some(index))?;
        p::apply(&mut state, &frames, frames.len())?;
        p::check(&mut state)?;
        drop((log, state));
        let before = p::cohort(&dir)?;
        let logs = f::log_rows(&dir)?;
        let (log, state) = f::open_pair(&dir)?;
        assert_eq!(
            recover_committed(log, state, s.group(), s.recovery(), p::selected(true)?).err(),
            Some(E::Codec(CodecError::TrailingBytes))
        );
        assert_eq!(p::cohort(&dir)?, before);
        assert_eq!(f::log_rows(&dir)?, logs);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}
