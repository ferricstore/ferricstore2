//! A Flow's key and the hash map the shard's indexes use (ADR-015).

use std::collections::HashMap;
use std::sync::Arc;

use wf_application::commands::queue::Partition;

/// A hash map on the shard's hot paths: foldhash, randomly seeded per map,
/// several times cheaper than `SipHash` on short keys (ADR-015).
pub(crate) type Map<K, V> = HashMap<K, V, foldhash::fast::RandomState>;

/// A Flow's partition and id, shared by the records table and every index
/// that names the Flow, so an index entry costs a reference count instead
/// of a copy of the id (ADR-015).
pub(crate) type Key = Arc<(Partition, Vec<u8>)>;

/// A Flow's partition and id for a lookup; the records table finds a
/// [`Key`] by it without allocating one.
pub(crate) type Parts = (Partition, Vec<u8>);

/// The key of the Flow `id` in `partition`.
pub(crate) fn flow_key(partition: Partition, id: Vec<u8>) -> Key {
    Arc::new((partition, id))
}
