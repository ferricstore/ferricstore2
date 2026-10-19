//! Exact genesis inventory; returns the same database for the reopen assertions.

use redb::{ReadableDatabase, TableHandle};
use std::path::Path;

use super::TestResult;

pub(super) fn check(path: &Path) -> TestResult<redb::Database> {
    let database = redb::Builder::new().open(path)?;
    let read = database.begin_read()?;
    let mut names = read
        .list_tables()?
        .map(|table| table.name().to_owned())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        [
            "wf_state_meta_v1",
            "wf_state_rows_v1",
            "wf_state_snapshot_meta_v1",
            "wf_state_snapshot_rows_v1"
        ]
    );
    assert_eq!(read.list_multimap_tables()?.count(), 0);
    drop(read);
    Ok(database)
}
