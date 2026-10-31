//! Real commit uncertainty, actual snapshot/replay and receipt-only capability closure.
use super::*;
use crate::workflow_values::claim::fixtures as c;
use wf_storage::transaction::{
    RecordBatchTransactionError as B, WorkflowValueTransactionError as V,
};

fn seeded(path: &Path) -> TestResult {
    fixture(path)?;
    edit(path, &[(key(20, None, b"i")?, binding(b"i")?)], &[])
}

#[test]
fn workflow_release_sync_uncertainty_keeps_one_complete_cohort_and_fences() -> TestResult {
    use std::sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    };
    let dir = crate::fixtures::directory("release-atomic-uncertain")?;
    let reference = dir.join("reference");
    let path = dir.join("state");
    let scope = crate::fixtures::scope()?;
    let prepared = frame(b"cancel", 5, &[b"i"])?;
    seeded(&reference)?;
    let mut expected = open(&reference)?;
    apply_workflow_release_transaction(
        &mut expected,
        &scope,
        prepared.as_bytes(),
        crate::fixtures::log(1),
        f::limits(),
    )?;
    drop(expected);
    let complete = c::cohort(&reference)?;
    seeded(&path)?;
    let old = c::cohort(&path)?;
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = crate::faults::wrapped(&path, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    assert_eq!(
        apply_workflow_release_transaction(
            &mut store,
            &scope,
            prepared.as_bytes(),
            crate::fixtures::log(1),
            f::limits()
        )
        .err(),
        Some(V::Record(B::Transaction(E::Uncertain)))
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    assert!(matches!(
        store.begin_apply(
            scope.group(),
            scope.recovery(),
            f::limits().record_batch.codec.admission(),
            1000,
            65536,
            65536
        ),
        Err(E::Unavailable)
    ));
    drop(store);
    let actual = c::cohort(&path)?;
    assert!(actual == old || actual == complete);
    let mut reopened = open(&path)?;
    let page = reopened.read_workflow_release_outbox(&scope, None, 256, 1000, 65536)?;
    let record = c::record(&mut reopened, b"i")?;
    assert_eq!(
        page.entries.len(),
        usize::try_from(record.record.version() - 1)?
    );
    drop(reopened);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn workflow_release_actual_commit_snapshot_restores_original_receipt_and_pending_intent()
-> TestResult {
    use std::io::Cursor;
    use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
    use wf_storage::snapshot::RedbSnapshotStage;
    let dir = crate::fixtures::directory("release-atomic-snapshot")?;
    let path = dir.join("state");
    let target = dir.join("received");
    seeded(&path)?;
    let scope = crate::fixtures::scope()?;
    let prepared = frame(b"cancel", 5, &[b"i"])?;
    let mut store = open(&path)?;
    let original = apply_workflow_release_transaction(
        &mut store,
        &scope,
        prepared.as_bytes(),
        crate::fixtures::log(1),
        f::limits(),
    )?;
    let O::Retained { outcome, .. } = original else {
        return Err("missing original".into());
    };
    let mut wire = Vec::new();
    store.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    drop(store);
    let stage = RedbSnapshotStage::receive(
        &target,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let mut restored = stage.into_store()?;
    let page = restored.read_workflow_release_outbox(&scope, None, 256, 1000, 65536)?;
    assert_eq!(page.meta.tail, 1);
    assert_eq!(page.entries[0].intent()?.reservation_id, b"paid\0id");
    let replay = apply_workflow_release_transaction(
        &mut restored,
        &scope,
        prepared.as_bytes(),
        crate::fixtures::log(2),
        f::limits(),
    )?;
    let O::Retained {
        outcome: replay, ..
    } = replay
    else {
        return Err("missing restored replay".into());
    };
    assert_eq!(*outcome, *replay);
    assert_eq!(
        restored
            .read_workflow_release_outbox(&scope, None, 256, 1000, 65536)?
            .meta
            .tail,
        1
    );
    drop(restored);
    drop(open(&target)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn workflow_release_resource_refusal_has_no_partial_queue_or_workflow_publication() -> TestResult {
    for steps in [1, 20, 50] {
        let dir = crate::fixtures::directory("release-atomic-admission")?;
        let path = dir.join("state");
        seeded(&path)?;
        let before = c::cohort(&path)?;
        let mut store = open(&path)?;
        let prepared = frame(b"cancel", 5, &[b"i"])?;
        let mut limits = f::limits();
        limits.record_batch.maximum_read_steps = steps;
        assert_eq!(
            apply_workflow_release_transaction(
                &mut store,
                &crate::fixtures::scope()?,
                prepared.as_bytes(),
                crate::fixtures::log(1),
                limits
            )
            .err(),
            Some(V::Record(B::Transaction(E::Limit)))
        );
        drop(store);
        assert_eq!(c::cohort(&path)?, before);
        drop(open(&path)?);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

pub(super) fn full_queue_controls(path: &Path) -> TestResult {
    receipt_only_boundary(path)?;
    let scope = crate::fixtures::scope()?;
    let mut store = open(path)?;
    for (request, profile, index) in [(b"full6".as_slice(), 6, 2), (b"full1", 1, 3)] {
        let prepared = frame(request, profile, &[b"i"])?;
        let applied = apply_workflow_release_transaction(
            &mut store,
            &scope,
            prepared.as_bytes(),
            crate::fixtures::log(index),
            f::limits(),
        )?;
        let O::Retained { outcome, .. } = applied else {
            return Err("missing whole rejection".into());
        };
        assert_eq!(
            outcome.status(),
            RetainedStatus::IndexedRejected(
                wf_domain::workflow::indexed::IndexRejection::ReleaseBacklogFull
            )
        );
        assert_eq!(outcome.response(), [1, 8, 0]);
    }
    let prepared = frame(b"full", 5, &[b"i"])?;
    let replay = apply_workflow_release_transaction(
        &mut store,
        &scope,
        prepared.as_bytes(),
        crate::fixtures::log(4),
        f::limits(),
    )?;
    assert!(
        matches!(replay, O::Retained { outcome, .. } if outcome.original_position() == crate::fixtures::log(1))
    );
    drop(store);
    edit(path, &[], &[key(20, None, b"i")?])?;
    let mut store = open(path)?;
    let prepared = frame(b"unbound", 1, &[b"i"])?;
    let applied = apply_workflow_release_transaction(
        &mut store,
        &scope,
        prepared.as_bytes(),
        crate::fixtures::log(5),
        f::limits(),
    )?;
    assert!(
        matches!(applied, O::Retained { outcome, .. } if outcome.status() == RetainedStatus::Accepted)
    );
    assert_eq!(
        store
            .read_workflow_release_outbox(&scope, None, 1, 1000, 65536)?
            .meta
            .tail,
        65536
    );
    drop(store);
    Ok(())
}

fn receipt_only_boundary(source: &Path) -> TestResult {
    let scope = crate::fixtures::scope()?;
    let target = source.with_extension("gc3-receipt");
    let placements = [(PartitionId::new(b"p".to_vec())?, scope.group().clone())];
    let mut store = RedbStateStore::create_governance_prune_database(
        redb::Builder::new().create(&target)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: crate::fixtures::clock(0)?,
            placements: &placements,
            policies: &[],
        },
        f::defaults(),
        Some(4),
        StateStoreLimits::default(),
    )?;
    c::create(&mut store, b"i", b"q", (0, 100, 0), Some(b"payload"))?;
    drop(store);
    let old = c::cohort(&target)?;
    let mut store = open(&target)?;
    let prepared = frame(b"old-refusal", 5, &[b"i"])?;
    assert_eq!(
        apply_workflow_release_transaction(
            &mut store,
            &scope,
            prepared.as_bytes(),
            crate::fixtures::log(1),
            f::limits()
        )
        .err(),
        Some(V::Record(B::Transaction(E::UnsupportedPolicyProfile)))
    );
    drop(store);
    assert_eq!(c::cohort(&target)?, old);
    let rows = crate::fixtures::raw_rows(source)?;
    let outcome_key = wf_storage::state::key::encode_outcome_key(
        &scope,
        &wf_types::identity::RequestId::new(b"full".to_vec())?,
        65536,
    )?;
    let control = rows
        .iter()
        .find(|(key, _)| key[6] == 0)
        .ok_or("missing control")?;
    edit(
        &target,
        &[
            (control.0.clone(), control.1.clone()),
            (
                outcome_key.clone(),
                rows.get(&outcome_key)
                    .ok_or("missing full rejection")?
                    .clone(),
            ),
        ],
        &[],
    )?;
    let before = c::cohort(&target)?;
    assert_eq!(open(&target).err(), Some(E::Corrupt));
    assert_eq!(c::cohort(&target)?, before);
    edit(&target, &[], &[outcome_key])?;
    let compatible = open(&target)?;
    assert_eq!(compatible.governance_catalog_profile(), 3);
    drop(compatible);
    Ok(())
}

#[test]
fn workflow_release_exhausted_sequence_is_operational_and_leaves_no_business_receipt() -> TestResult
{
    let dir = crate::fixtures::directory("release-atomic-exhausted")?;
    let path = dir.join("state");
    seeded(&path)?;
    let max = wf_domain::workflow::MAX_EXACT_MILLIS;
    edit(
        &path,
        &[(
            key(21, None, b"")?,
            encode_release_meta(
                ReleaseOutboxState {
                    head: max + 1,
                    tail: max,
                },
                22,
            )?,
        )],
        &[],
    )?;
    let before = c::cohort(&path)?;
    let mut store = open(&path)?;
    let scope = crate::fixtures::scope()?;
    let prepared = frame(b"cancel", 5, &[b"i"])?;
    assert_eq!(
        apply_workflow_release_transaction(
            &mut store,
            &scope,
            prepared.as_bytes(),
            crate::fixtures::log(1),
            f::limits()
        )
        .err(),
        Some(V::Record(B::Transaction(E::Corrupt)))
    );
    assert!(matches!(
        store.begin_apply(
            scope.group(),
            scope.recovery(),
            f::limits().record_batch.codec.admission(),
            1000,
            65536,
            65536
        ),
        Err(E::Unavailable)
    ));
    drop(store);
    assert_eq!(c::cohort(&path)?, before);
    drop(open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn workflow_release_writer_cannot_turn_a_legacy_claim_into_paid_authority() -> TestResult {
    let dir = crate::fixtures::directory("release-atomic-claim-refused")?;
    let path = dir.join("state");
    seeded(&path)?;
    let before = c::cohort(&path)?;
    let mut store = open(&path)?;
    let scope = crate::fixtures::scope()?;
    let claim = f::item(
        b"i",
        200,
        Operation::Claim(wf_domain::claim::Claim {
            expected_state: b"q".to_vec(),
            worker: b"worker".to_vec(),
            token: wf_domain::lease::LeaseToken::new(b"token".to_vec())?,
            lease_ms: 1000,
        }),
    )?;
    let batch = f::batch(
        5,
        b"claim",
        200,
        PreparedOperation::Batch(vec![claim]),
        vec![f::input(0, 0)],
        vec![f::witness(b"r", None)?],
    )?;
    assert_eq!(
        encode_workflow_stored_value_batch(&batch, f::limits().record_batch.codec).err(),
        Some(wf_ports::log::CodecError::UnsupportedTag)
    );
    let prepared = c::phase(b"claim", 200, b"q", b"worker", 1)?;
    assert_eq!(
        apply_workflow_release_transaction(
            &mut store,
            &scope,
            prepared.as_bytes(),
            crate::fixtures::log(1),
            f::limits()
        )
        .err(),
        Some(V::Record(B::Apply(
            wf_ports::log::RecordBatchApplyError::Encoding(
                wf_ports::log::CodecError::UnsupportedVersion
            )
        )))
    );
    drop(store);
    assert_eq!(c::cohort(&path)?, before);
    drop(open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn workflow_release_mutation_entrypoint_does_not_replace_the_claim_parent() -> TestResult {
    let dir = crate::fixtures::directory("release-acquisition-boundary")?;
    let path = dir.join("state");
    fixture(&path)?;
    let before = c::cohort(&path)?;
    let mut store = open(&path)?;
    let scope = crate::fixtures::scope()?;
    let item = f::item(
        b"j",
        200,
        Operation::StartAndClaim(wf_domain::workflow::StartAndClaim {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            worker: b"worker".to_vec(),
            lease_ms: 1000,
            payload: None,
        }),
    )?;
    let batch = f::batch(
        7,
        b"start",
        200,
        PreparedOperation::Batch(vec![item]),
        vec![f::input(0, 0)],
        vec![f::witness(b"r", None)?],
    )?;
    let prepared = encode_workflow_stored_value_batch(&batch, f::limits().record_batch.codec)?;
    assert_eq!(
        apply_workflow_release_transaction(
            &mut store,
            &scope,
            prepared.as_bytes(),
            crate::fixtures::log(1),
            f::limits()
        )
        .err(),
        Some(V::UnsupportedProfile)
    );
    drop(store);
    assert_eq!(c::cohort(&path)?, before);
    let mut store = open(&path)?;
    let legacy = wf_storage::transaction::apply_workflow_stored_value_batch_transaction(
        &mut store,
        &scope,
        prepared.as_bytes(),
        crate::fixtures::log(1),
        f::limits(),
    )?;
    assert!(
        matches!(legacy, O::Retained { outcome, .. } if outcome.status() == RetainedStatus::Accepted)
    );
    assert_eq!(c::record(&mut store, b"j")?.record.state(), b"running");
    drop(store);
    drop(open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
