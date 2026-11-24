//! Finished Flows on disk when the node opens (ADR-016): every shard shares
//! one cold store and its writer thread, and after replay the node waits
//! for replay's finished Flows to reach disk, then removes the cold files no
//! stub names (written after the last snapshot and written again by
//! replay, or emptied by retention).

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use wf_storage::cold::{ColdQueue, ColdStore};

use crate::supervision::shard::state::ShardState;

use super::{NodeError, QueueNodeConfig};

/// How long an opening node waits for replay's finished Flows to reach
/// disk before it tidies the cold files.
const SETTLE: Duration = Duration::from_secs(60);

/// The cold store and one empty state per shard using it.
pub(super) fn shards(
    directory: &Path,
    config: &QueueNodeConfig,
) -> Result<(Arc<ColdStore>, Vec<ShardState>), NodeError> {
    let cold = ColdStore::open(&directory.join("cold")).map_err(|_| NodeError::Cold)?;
    let queue = Arc::new(ColdQueue::spawn(&cold).map_err(|_| NodeError::Thread)?);
    let states = (0..config.shards)
        .map(|_| ShardState::new(config.retention_ms, (Arc::clone(&cold), Arc::clone(&queue))))
        .collect();
    Ok((cold, states))
}

/// Moves replay's finished Flows to disk and removes cold files no stub
/// names; those the loaded snapshot names (`named`) wait for the next
/// snapshot when the store keeps them for snapshots.
pub(super) fn tidy(
    cold: &ColdStore,
    states: &mut [ShardState],
    named: &HashSet<u64>,
) -> Result<(), NodeError> {
    for state in states.iter_mut() {
        state.cool();
    }
    let mut referenced = HashSet::new();
    for state in states.iter_mut() {
        state.settle_cold(SETTLE);
        referenced.extend(state.cold_files());
    }
    cold.remove_unreferenced(&referenced, named)
        .map_err(|_| NodeError::Cold)
}
