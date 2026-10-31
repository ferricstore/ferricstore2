//! Complete typed disk cohorts and real snapshot/missing-owner controls.

use super::*;

impl PublicationFixture {
    pub(in crate::catalog_publication) fn remove_owner(
        &self,
        index: usize,
        name: &[u8],
    ) -> TestResult {
        let path = self.find(&self.sources[index])?.path.join("state.redb");
        let db = redb::Builder::new().open(path)?;
        let tx = db.begin_write()?;
        {
            let mut table =
                tx.open_table(TableDefinition::<&[u8], &[u8]>::new("wf_state_rows_v1"))?;
            let mut selected = None;
            for row in table.iter()? {
                let (key, value) = row?;
                if key.value().get(6) == Some(&12)
                    && wf_ports::log::decode_credit_owner(
                        value.value(),
                        CreditLimits::default(),
                        900000,
                        32 * f::MIB,
                    )
                    .is_ok_and(|owner| owner.view().scope == name)
                {
                    selected = Some(key.value().to_vec());
                    break;
                }
            }
            let key = selected.ok_or("missing test owner")?;
            assert!(table.remove(key.as_slice())?.is_some());
        }
        tx.commit()?;
        Ok(())
    }
    pub(in crate::catalog_publication) fn cohort(
        &self,
        index: usize,
    ) -> TestResult<Vec<(String, f::Rows)>> {
        let path = &self.find(&self.sources[index])?.path;
        let mut result = Vec::new();
        for (prefix, file) in [("state", "state.redb"), ("log", "log.redb")] {
            let db = redb::Builder::new().open(path.join(file))?;
            let tx = db.begin_read()?;
            assert_eq!(tx.list_multimap_tables()?.count(), 0);
            for handle in tx.list_tables()? {
                let name = handle.name();
                let mut rows = f::Rows::new();
                if name == "wf_raft_log_v1" {
                    let table = tx.open_table(TableDefinition::<u64, &[u8]>::new(name))?;
                    for row in table.iter()? {
                        let (k, v) = row?;
                        assert!(
                            rows.insert(k.value().to_be_bytes().to_vec(), v.value().to_vec())
                                .is_none()
                        );
                    }
                } else {
                    let table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
                    for row in table.iter()? {
                        let (k, v) = row?;
                        assert!(
                            rows.insert(k.value().to_vec(), v.value().to_vec())
                                .is_none()
                        );
                    }
                }
                result.push((format!("{prefix}/{name}"), rows));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(result.len(), 8);
        Ok(result)
    }
    pub(in crate::catalog_publication) fn verify_all(&self) -> TestResult {
        for store in &self.stores {
            let (log, state, profile) = self.open(&store.scope)?;
            drop(recover_snapshotted(
                log,
                state,
                store.scope.group(),
                store.scope.recovery(),
                caps(profile)?,
                SnapshotLimits::default(),
            )?);
        }
        Ok(())
    }
    pub(in crate::catalog_publication) fn snapshot_roundtrip(&self) -> TestResult {
        let source = &self.sources[0];
        let (log, mut state, profile) = self.open(source)?;
        let mut wire = Vec::new();
        state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
        let (log, state) = purge_snapshot(
            log,
            state,
            source.group(),
            source.recovery(),
            caps(profile)?,
            SnapshotLimits::default(),
        )?;
        drop(recover_snapshotted(
            log,
            state,
            source.group(),
            source.recovery(),
            caps(profile)?,
            SnapshotLimits::default(),
        )?);
        let received = self.root.join("received");
        std::fs::create_dir(&received)?;
        let log = RedbLogStore::create(
            &received.join("log.redb"),
            source.group().clone(),
            source.recovery().clone(),
            RaftLogLimits::default(),
        )?;
        let candidate = RedbSnapshotStage::receive(
            &received.join("state.redb"),
            source.group().clone(),
            source.recovery().clone(),
            &mut std::io::Cursor::new(wire),
            StateStoreLimits::default(),
            SnapshotLimits::default(),
        )?;
        drop(restore_received_snapshot(
            log,
            candidate,
            source.group(),
            source.recovery(),
            caps(profile)?,
            SnapshotLimits::default(),
        )?);
        Ok(())
    }
    pub(in crate::catalog_publication) fn cleanup(self) -> TestResult {
        std::fs::remove_dir_all(self.root)?;
        Ok(())
    }
}
