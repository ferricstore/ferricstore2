//! Actual paid publication failure cuts and receipt-only capability closure.
use super::*;
use wf_ports::state::StateTransactionError as S;

#[test]
fn paid_claim_uncertain_sync_fences_and_recovers_complete_old_or_new_cohort() -> TestResult {
    use std::sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    };
    let dir = f::directory("paid-claim-uncertain")?;
    let path = dir.join("state");
    let reference = dir.join("reference");
    drop(paid(&path)?);
    let old = fixtures::cohort(&path)?;
    let prepared = frame(b"claim", 1003, &[b"a".to_vec(), b"b".to_vec()])?;
    let mut expected = paid(&reference)?;
    apply(&mut expected, &prepared, 3)?;
    drop(expected);
    let complete = fixtures::cohort(&reference)?;
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = crate::faults::wrapped(&path, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    assert_eq!(
        apply_governed_claim_transaction(
            &mut store,
            &f::scope()?,
            prepared.as_bytes(),
            f::log(3),
            v::limits(),
            ClaimWorkLimits::default()
        )
        .err(),
        Some(wf_storage::transaction::ClaimTransactionError::Transaction(
            S::Uncertain
        ))
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    let scope = f::scope()?;
    assert!(matches!(
        store.begin_apply(
            scope.group(),
            scope.recovery(),
            v::limits().record_batch.codec.admission(),
            1000,
            1024 * 1024,
            1024 * 1024
        ),
        Err(S::Unavailable)
    ));
    drop(store);
    let actual = fixtures::cohort(&path)?;
    assert!(actual == old || actual == complete);
    let mut restored = f::open(&path)?;
    let version = fixtures::record(&mut restored, b"i")?.record.version();
    drop(restored);
    assert_eq!(
        f::raw_rows(&path)?
            .keys()
            .filter(|key| key.get(6) == Some(&24))
            .count(),
        usize::try_from(version - 1)?
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn paid_claim_operational_refusal_preserves_full_cohort_without_business_receipt() -> TestResult {
    let dir = f::directory("paid-claim-admission")?;
    let path = dir.join("state");
    let mut store = paid(&path)?;
    let prepared = frame(b"claim", 1003, &[b"a".to_vec(), b"b".to_vec()])?;
    drop(store);
    let before = fixtures::cohort(&path)?;
    store = f::open(&path)?;
    let mut limits = v::limits();
    limits.record_batch.maximum_read_steps = 1;
    assert_eq!(
        apply_governed_claim_transaction(
            &mut store,
            &f::scope()?,
            prepared.as_bytes(),
            f::log(3),
            limits,
            ClaimWorkLimits::default()
        )
        .err(),
        Some(wf_storage::transaction::ClaimTransactionError::Transaction(
            S::Limit
        ))
    );
    drop(store);
    assert_eq!(fixtures::cohort(&path)?, before);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn paid_receipt_requires_gc5_even_without_bindings_or_consumed_markers() -> TestResult {
    use redb::ReadableTable;
    use sha2::{Digest, Sha256};
    let dir = f::directory("paid-claim-receipt-custody")?;
    let path = dir.join("state");
    let mut store = paid(&path)?;
    let original = apply(
        &mut store,
        &frame(b"claim", 1003, &[b"missing".to_vec()])?,
        3,
    )?;
    assert!(matches!(
        original.status(),
        RetainedStatus::ClaimRejected(_)
    ));
    assert!(
        wf_storage::state::value::encode_outcome(&original, 1024 * 1024).is_err(),
        "legacy scalar receipt encoding must remain closed to paid Claim"
    );
    drop(store);
    assert_eq!(
        f::raw_rows(&path)?
            .keys()
            .filter(|key| matches!(key.get(6), Some(20 | 24)))
            .count(),
        0
    );
    let db = redb::Database::open(&path)?;
    let tx = db.begin_write()?;
    {
        let mut metadata = tx.open_table(redb::TableDefinition::<&[u8], &[u8]>::new(
            "wf_state_meta_v1",
        ))?;
        let mut bytes = metadata
            .get(b"manifest".as_slice())?
            .ok_or("manifest")?
            .value()
            .to_vec();
        let at = bytes.len() - 36;
        assert_eq!(&bytes[at..at + 4], b"GC\0\x05");
        bytes[at + 3] = 4;
        let end = bytes.len() - 32;
        let checksum = Sha256::digest(&bytes[..end]);
        bytes[end..].copy_from_slice(&checksum);
        metadata.insert(b"manifest".as_slice(), bytes.as_slice())?;
    }
    tx.commit()?;
    drop(db);
    let before = fixtures::cohort(&path)?;
    let scope = f::scope()?;
    assert_eq!(
        RedbStateStore::open(
            &path,
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreLimits::default()
        )
        .err(),
        Some(S::Corrupt)
    );
    assert_eq!(fixtures::cohort(&path)?, before);
    let receipt = wf_storage::state::key::encode_outcome_key(
        &scope,
        &RequestId::new(b"claim".to_vec())?,
        65535,
    )?;
    let db = redb::Database::open(&path)?;
    let tx = db.begin_write()?;
    {
        let mut rows = tx.open_table(redb::TableDefinition::<&[u8], &[u8]>::new(
            "wf_state_rows_v1",
        ))?;
        assert!(rows.remove(receipt.as_slice())?.is_some());
    }
    tx.commit()?;
    drop(db);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
