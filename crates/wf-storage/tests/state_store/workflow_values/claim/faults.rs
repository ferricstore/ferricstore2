use super::{fixtures as s, *};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError};
use wf_storage::transaction::ClaimTransactionError;

#[test]
fn named_value_claim_uncertain_commit_fences_and_reopens_one_complete_cohort() -> TestResult {
    let dir = f::directory("named-value-claim-uncertain")?;
    let reference = dir.join("reference");
    let frame = s::phase(b"claim", 2, b"q", b"w", 1)?;
    let mut expected = s::fresh(&reference, false)?;
    s::create(&mut expected, b"i", b"q", (0, 0, 0), Some(b"A"))?;
    s::retained(s::apply(&mut expected, &frame, 1)?)?;
    drop(expected);
    let complete = s::cohort(&reference)?;
    let path = dir.join("state");
    let mut store = s::fresh(&path, false)?;
    s::create(&mut store, b"i", b"q", (0, 0, 0), Some(b"A"))?;
    drop(store);
    let old = s::cohort(&path)?;
    let fault = Arc::new(AtomicU8::new(0));
    let mut wrapped = crate::faults::wrapped(&path, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    assert_eq!(
        apply_workflow_value_claim_transaction(
            &mut wrapped,
            &f::scope()?,
            frame.as_bytes(),
            f::log(1),
            s::limits(),
            ClaimWorkLimits::default()
        )
        .err(),
        Some(ClaimTransactionError::Transaction(
            StateTransactionError::Uncertain
        ))
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    let scope = f::scope()?;
    assert!(matches!(
        wrapped.begin_apply(
            scope.group(),
            scope.recovery(),
            s::limits().record_batch.codec.admission(),
            10000,
            64 * f::MIB,
            32 * f::MIB
        ),
        Err(StateTransactionError::Unavailable)
    ));
    drop(wrapped);
    let actual = s::cohort(&path)?;
    assert!(actual == old || actual == complete);
    let mut reopened = f::open(&path)?;
    let version = s::record(&mut reopened, b"i")?.record.version();
    assert!([1, 2].contains(&version));
    let control = reopened
        .begin_apply(
            scope.group(),
            scope.recovery(),
            s::limits().record_batch.codec.admission(),
            10000,
            64 * f::MIB,
            32 * f::MIB,
        )?
        .control()?;
    assert_eq!(control.position().get(), version - 1);
    assert_eq!(
        s::history(&reopened, b"i")?.entries.len(),
        usize::try_from(version)?
    );
    let acknowledged = s::retained(s::apply(&mut reopened, &frame, version)?)?;
    assert_eq!(acknowledged.status(), RetainedStatus::Accepted);
    assert_eq!(acknowledged.original_position(), f::log(1));
    assert_eq!(s::record(&mut reopened, b"i")?.record.version(), 2);
    assert_eq!(s::history(&reopened, b"i")?.entries.len(), 2);
    drop(reopened);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn named_value_claim_snapshot_receive_preserves_all_value_and_history_rows() -> TestResult {
    use std::io::Cursor;
    use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
    use wf_storage::snapshot::RedbSnapshotStage;
    use wf_storage::state::StateStoreLimits;
    let dir = f::directory("named-value-claim-snapshot")?;
    let source = dir.join("source");
    let mut store = s::fresh(&source, true)?;
    s::create(&mut store, b"i", b"q", (0, 0, 0), Some(b"FSV2\0\xff"))?;
    let frame = s::phase(b"claim", 2, b"q", b"w", 1)?;
    let original = s::retained(s::apply(&mut store, &frame, 1)?)?;
    let record = s::record(&mut store, b"i")?;
    let mut wire = Vec::new();
    let captured = store.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    assert!(wire.starts_with(b"FWSP\0\x04"));
    drop(store);
    let expected = s::cohort(&source)?;
    let scope = f::scope()?;
    let candidate = RedbSnapshotStage::receive(
        &dir.join("stage"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let target = dir.join("target");
    let mut restored = s::fresh(&target, true)?;
    assert_eq!(
        restored
            .install_snapshot(candidate, SnapshotLimits::default())?
            .id(),
        captured.id()
    );
    assert_eq!(s::record(&mut restored, b"i")?, record);
    assert_eq!(s::history(&restored, b"i")?.entries.len(), 2);
    drop(restored);
    assert_eq!(s::cohort(&target)?[1], expected[1]);
    let mut restored = f::open(&target)?;
    assert_eq!(
        s::retained(s::apply(&mut restored, &frame, 2)?)?.as_ref(),
        original.as_ref()
    );
    assert_eq!(s::record(&mut restored, b"i")?, record);
    drop(restored);
    drop(f::open(&target)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
