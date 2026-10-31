//! Capability and applied-state closure for the original release data rows.
use super::*;
use redb::ReadableTable;
use sha2::{Digest, Sha256};

#[test]
fn workflow_release_rows_refuse_checksum_valid_gc3_downgrade_without_writes() -> TestResult {
    let dir = crate::fixtures::directory("release-data-downgrade")?;
    let path = dir.join("state");
    fixture(&path)?;
    queue(&path)?;
    let db = redb::Database::open(&path)?;
    let tx = db.begin_write()?;
    {
        let mut metadata = tx.open_table(redb::TableDefinition::<&[u8], &[u8]>::new(
            "wf_state_meta_v1",
        ))?;
        let mut bytes = metadata
            .get(b"manifest".as_slice())?
            .ok_or("missing manifest")?
            .value()
            .to_vec();
        let at = bytes.len() - 36;
        assert_eq!(&bytes[at..at + 4], b"GC\0\x04");
        bytes[at + 3] = 3;
        let end = bytes.len() - 32;
        let hash = Sha256::digest(&bytes[..end]);
        bytes[end..].copy_from_slice(&hash);
        metadata.insert(b"manifest".as_slice(), bytes.as_slice())?;
    }
    tx.commit()?;
    drop(db);
    let before = super::super::claim::fixtures::cohort(&path)?;
    assert_eq!(open(&path).err(), Some(E::Corrupt));
    assert_eq!(super::super::claim::fixtures::cohort(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn workflow_release_rows_require_actual_applied_control() -> TestResult {
    let dir = crate::fixtures::directory("release-data-unapplied")?;
    let path = dir.join("state");
    let scope = crate::fixtures::scope()?;
    let store = RedbStateStore::create_workflow_release_database(
        redb::Builder::new().create(&path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: crate::fixtures::clock(0)?,
            placements: &[],
            policies: &[],
        },
        f::defaults(),
        4,
        StateStoreLimits::default(),
    )?;
    let page = store.read_workflow_release_outbox(&scope, None, 256, 1000, 65536)?;
    assert_eq!(page.entries.len(), 0);
    assert_eq!((page.meta.head, page.meta.tail, page.more), (1, 0, false));
    drop(store);
    edit(
        &path,
        &[(
            key(21, None, b"")?,
            encode_release_meta(ReleaseOutboxState { head: 1, tail: 0 }, 22)?,
        )],
        &[],
    )?;
    let before = super::super::claim::fixtures::cohort(&path)?;
    assert_eq!(open(&path).err(), Some(E::Corrupt));
    assert_eq!(super::super::claim::fixtures::cohort(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn workflow_release_reads_refuse_older_store_and_foreign_namespace() -> TestResult {
    use wf_ports::state::GroupApplyStore;
    let dir = crate::fixtures::directory("release-read-closed")?;
    let path = dir.join("state");
    let mut store = f::fresh(&path)??.upgrade_history_snapshot_layout()?;
    let scope = crate::fixtures::scope()?;
    assert_eq!(
        store
            .read_workflow_release_outbox(&scope, None, 1, 1000, 65536)
            .err(),
        Some(E::UnsupportedPolicyProfile)
    );
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        f::limits().record_batch.codec.admission(),
        1000,
        65536,
        65536,
    )?;
    assert_eq!(
        tx.workflow_governance_binding(
            &scope,
            &PartitionId::new(b"p".to_vec())?,
            &wf_types::identity::RecordId::new(b"i".to_vec())?
        )
        .err(),
        Some(E::UnsupportedPolicyProfile)
    );
    drop(tx);
    drop(store);
    let gc4 = dir.join("gc4");
    fixture(&gc4)?;
    let store = open(&gc4)?;
    let foreign = wf_domain::workflow::request::RequestScope::new(
        scope.tenant().clone(),
        wf_types::identity::GroupId::new(b"foreign".to_vec())?,
        scope.recovery().clone(),
    );
    assert_eq!(
        store
            .read_workflow_release_outbox(&foreign, None, 1, 1000, 65536)
            .err(),
        Some(E::NamespaceConflict)
    );
    drop(store);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
