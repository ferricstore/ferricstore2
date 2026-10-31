//! Bounded flat cursor ownership; source text is interpreted without UTF-8 copies.
use super::E;
/// Validated mathematical cursor; it is not a reusable read or retention permit.
pub struct ReleaseReconcileCursor {
    next: u64,
    shards: u64,
    positions: Vec<(u64, u64)>,
}
impl ReleaseReconcileCursor {
    /// Validates a complete original position cohort before allocating descriptors.
    /// # Errors
    /// Refuses numeric/duplicate/count data, prospective owner bounds or allocation.
    pub fn from_parts(
        next: u64,
        positions: &[(u64, u64)],
        shards: u64,
        maximum_owned: usize,
    ) -> Result<Self, E> {
        let _ = (next, positions, shards, maximum_owned);
        Err(E::Malformed)
    }
    /// Original next selected source shard.
    #[must_use]
    pub const fn next_shard(&self) -> u64 {
        self.next
    }
    /// Configured original source count, without placement authority.
    #[must_use]
    pub const fn shard_count(&self) -> u64 {
        self.shards
    }
    /// Canonical ascending per-source positions, without a copied map.
    #[must_use]
    pub fn positions(&self) -> &[(u64, u64)] {
        &self.positions
    }
}
pub(super) fn decode(
    encoded: Option<&[u8]>,
    shards: u64,
    maximum_owned: usize,
) -> Result<ReleaseReconcileCursor, E> {
    let _ = (encoded, shards, maximum_owned);
    Err(E::UnsupportedTag)
}
pub(super) fn encode(cursor: &ReleaseReconcileCursor, maximum: usize) -> Result<Vec<u8>, E> {
    let _ = (cursor, maximum);
    Err(E::UnsupportedTag)
}
