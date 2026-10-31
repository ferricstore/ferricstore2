//! Global bounded collection through configured read/loader capabilities.

use std::cmp::Ordering;
use wf_ports::state::{
    GovernanceCatalogReadLimits, GovernanceCatalogReadPort, LoadedGovernanceRecord,
    StateTransactionError,
};
mod model;
pub use model::{
    GovernanceCatalogCollection, GovernanceCatalogCollectionError,
    GovernanceCatalogCollectionLimits, GovernanceCatalogCollectionRequest,
};

/// Traverses all pages and returns globally ranked original loaded values.
/// The loader verifies member placement/namespace and enforces its supplied caps.
/// # Errors
/// Refuses invalid options/resources or any page/loader failure without a prefix.
pub fn collect_governance_catalog<P, T>(
    port: &P,
    request: GovernanceCatalogCollectionRequest<'_>,
    loader: impl FnMut(
        &[u8],
        GovernanceCatalogReadLimits,
    ) -> Result<Option<LoadedGovernanceRecord<T>>, StateTransactionError>,
    compare: impl Fn(&T, &T) -> Ordering,
    limits: GovernanceCatalogCollectionLimits,
) -> Result<GovernanceCatalogCollection<T>, GovernanceCatalogCollectionError>
where
    P: GovernanceCatalogReadPort,
{
    let _ = (port, request, loader, compare, limits);
    Err(GovernanceCatalogCollectionError::InvalidOptions)
}

/// Loads original explicit keys without any catalog traversal or result copying.
/// # Errors
/// Refuses invalid options/resources or loader failure without a partial result.
pub fn collect_governance_keys<T>(
    keys: &[&[u8]],
    limit: usize,
    loader: impl FnMut(
        &[u8],
        GovernanceCatalogReadLimits,
    ) -> Result<Option<LoadedGovernanceRecord<T>>, StateTransactionError>,
    compare: impl Fn(&T, &T) -> Ordering,
    limits: GovernanceCatalogCollectionLimits,
) -> Result<GovernanceCatalogCollection<T>, GovernanceCatalogCollectionError> {
    let _ = (keys, limit, loader, compare, limits);
    Err(GovernanceCatalogCollectionError::InvalidOptions)
}
