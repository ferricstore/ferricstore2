//! `OpenRaft`'s storage suite (`openraft::testing::log::Suite`) against the
//! entry log: log state, votes, appends, reads, truncation, purge and the
//! initial state after restarts, each on a fresh log.

use std::future::{Future, ready};
use std::sync::Arc;

use openraft::StorageError;
use openraft::testing::log::{StoreBuilder, Suite};
use openraft::type_config::TypeConfigExt;
use wf_storage::log::{EntryLog, GroupLogLimits};
use wf_test_support::resources::TestResources;

use super::machine::Machine;
use super::{TestResult, runtime};
use wf_consensus::group::{Config, LogStore};

struct Builder;

fn build() -> Result<(TestResources, LogStore, Machine), StorageError<Config>> {
    let failed = |error: &dyn std::error::Error| {
        StorageError::read(Config::err_from_error(&std::io::Error::other(
            error.to_string(),
        )))
    };
    let resources = TestResources::new().map_err(|error| failed(&error))?;
    let log = EntryLog::open(&resources.path().join("log"), GroupLogLimits::default())
        .map_err(|error| failed(&error))?;
    Ok((resources, LogStore::new(Arc::new(log)), Machine::default()))
}

impl StoreBuilder<Config, LogStore, Machine, TestResources> for Builder {
    fn build(
        &self,
    ) -> impl Future<Output = Result<(TestResources, LogStore, Machine), StorageError<Config>>> + Send
    {
        ready(build())
    }
}

#[test]
fn the_entry_log_passes_openraft_s_storage_suite() -> TestResult {
    runtime()?.block_on(Suite::test_all(Builder))?;
    Ok(())
}
