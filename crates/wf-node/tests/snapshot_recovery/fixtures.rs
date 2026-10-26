//! Whole four-table state plus both log tables, without a fake storage adapter.

use super::{
    CommittedRecoveryLimits, Error, RaftLogStore, SnapshotLimits, StateSnapshotStore, TestResult,
    f, purge_snapshot, recover_snapshotted,
};
use redb::{ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition, TableHandle};
use std::path::Path;
use wf_domain::workflow::control::EntryStamp;
use wf_storage::{log::RedbLogStore, state::RedbStateStore};

pub(super) const TABLES: [&str; 4] = [
    "wf_state_meta_v1",
    "wf_state_rows_v1",
    "wf_state_snapshot_meta_v1",
    "wf_state_snapshot_rows_v1",
];

pub(super) fn cohort(dir: &Path) -> TestResult<[f::Rows; 4]> {
    let database = redb::Builder::new().open(dir.join("state.redb"))?;
    let read = database.begin_read()?;
    let mut names = read
        .list_tables()?
        .map(|x| x.name().to_owned())
        .collect::<Vec<_>>();
    names.sort();
    let mut expected = TABLES.map(str::to_owned);
    expected.sort();
    assert_eq!(names, expected);
    assert_eq!(read.list_multimap_tables()?.count(), 0);
    let mut result = std::array::from_fn(|_| f::Rows::new());
    for (slot, name) in result.iter_mut().zip(TABLES) {
        let table = read.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
        for row in table.iter()? {
            let (k, v) = row?;
            assert!(
                slot.insert(k.value().to_vec(), v.value().to_vec())
                    .is_none()
            );
        }
        assert_eq!(u64::try_from(slot.len())?, table.len()?);
    }
    Ok(result)
}

pub(super) fn capture(state: &mut RedbStateStore) -> TestResult<[u8; 32]> {
    let metadata = state.capture_snapshot(&mut std::io::sink(), SnapshotLimits::default())?;
    Ok(*metadata.id())
}

pub(super) fn artifact(state: &mut RedbStateStore) -> TestResult<Vec<u8>> {
    let mut bytes = Vec::new();
    let metadata = state
        .current_snapshot(&mut bytes, SnapshotLimits::default())?
        .ok_or("missing real snapshot")?;
    assert_eq!(metadata.encoded_bytes(), bytes.len());
    Ok(bytes)
}

pub(super) fn seeded(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore, [u8; 32])> {
    let (mut log, mut state) = f::create_pair(dir)?;
    let member = f::member(EntryStamp::Stamped(f::clock(10, 0, 10)?), 4)?;
    f::append(&mut log, 0, member.as_bytes())?;
    f::apply_control(&mut state, 0, member.as_bytes())?;
    let id = capture(&mut state)?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::append(&mut log, 1, blank.as_bytes())?;
    f::apply_control(&mut state, 1, blank.as_bytes())?;
    log.save_committed(f::at(1))?;
    Ok((log, state, id))
}

pub(super) fn assert_refusal(
    dir: &Path,
    purge: bool,
    limits: &CommittedRecoveryLimits,
    snapshot: SnapshotLimits,
    expected: Error,
) -> TestResult {
    let state = cohort(dir)?;
    let meta = f::log_meta(dir)?;
    let rows = f::log_rows(dir)?;
    let s = f::scope()?;
    let (log, active) = f::open_pair(dir)?;
    let result = if purge {
        purge_snapshot(log, active, s.group(), s.recovery(), *limits, snapshot)
    } else {
        recover_snapshotted(log, active, s.group(), s.recovery(), *limits, snapshot)
    };
    let error = result.err().ok_or("invalid snapshot join accepted")?;
    assert_eq!(error, expected);
    // Error owns neither database: the exact same paths must immediately reopen.
    drop(f::open_pair(dir)?);
    assert_eq!(cohort(dir)?, state);
    assert_eq!(f::log_meta(dir)?, meta);
    assert_eq!(f::log_rows(dir)?, rows);
    Ok(())
}

pub(super) fn replace_control(
    dir: &Path,
    control: wf_domain::workflow::control::ControlState,
    frozen: bool,
) -> TestResult {
    let s = f::scope()?;
    let key = wf_storage::state::key::encode_control_key(s.group(), s.recovery(), f::MIB)?;
    let value = wf_storage::state::value::encode_control(s.group(), s.recovery(), control, f::MIB)?;
    let database = redb::Builder::new().open(dir.join("state.redb"))?;
    let mut tx = database.begin_write()?;
    tx.set_durability(redb::Durability::Immediate)?;
    {
        let mut table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new(
            TABLES[if frozen { 3 } else { 1 }],
        ))?;
        assert!(table.insert(key.as_slice(), value.as_slice())?.is_some());
    }
    tx.commit()?;
    Ok(())
}
