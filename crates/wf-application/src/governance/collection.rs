//! Global bounded collection through configured read/loader capabilities.
use std::cmp::Ordering;
use wf_ports::state::{
    GovernanceCatalogPage, GovernanceCatalogRead, GovernanceCatalogReadLimits,
    GovernanceCatalogReadPort, LoadedGovernanceRecord, StateTransactionError,
};
mod model;
mod rank;
pub use model::{
    GovernanceCatalogCollection, GovernanceCatalogCollectionError,
    GovernanceCatalogCollectionLimits, GovernanceCatalogCollectionRequest,
};

/// Traverses all pages and returns globally ranked original loaded values.
/// The configured loader verifies placement/namespace and enforces supplied caps.
/// The comparator must give a total order. Equal values keep reverse arrival order.
/// # Errors
/// Refuses invalid options/resources or any page/loader failure without a prefix.
pub fn collect_governance_catalog<P, T>(
    port: &P,
    request: GovernanceCatalogCollectionRequest<'_>,
    mut loader: impl FnMut(
        &[u8],
        GovernanceCatalogReadLimits,
    ) -> Result<Option<LoadedGovernanceRecord<T>>, StateTransactionError>,
    compare: impl Fn(&T, &T) -> Ordering,
    limits: GovernanceCatalogCollectionLimits,
) -> Result<GovernanceCatalogCollection<T>, GovernanceCatalogCollectionError>
where
    P: GovernanceCatalogReadPort,
{
    use GovernanceCatalogCollectionError as E;
    if request.catalog.len() > 65535 {
        return Err(E::InvalidOptions);
    }
    let mut result = rank::start(request.limit, limits)?;
    let mut cursor: Option<Vec<u8>> = None;
    loop {
        let mut page = port
            .governance_catalog_page(
                GovernanceCatalogRead {
                    scope: request.scope,
                    catalog: request.catalog,
                    cursor: cursor.as_deref(),
                    limit: 256,
                },
                limits.page,
            )
            .map_err(E::Source)?;
        valid_page(&page, cursor.as_deref(), limits)?;
        for key in &page.members {
            if let Some(candidate) = loader(key, limits.load).map_err(E::Source)? {
                rank::retain(&mut result, candidate, request.limit, &compare, limits)?;
            }
        }
        if !page.full {
            return Ok(result);
        }
        cursor = page.members.pop();
    }
}

/// Loads original explicit keys without any catalog traversal or result copying.
/// The configured comparator gives a total order, including reverse-arrival ties.
/// # Errors
/// Refuses invalid options/resources or loader failure without a partial result.
pub fn collect_governance_keys<T>(
    keys: &[&[u8]],
    limit: usize,
    mut loader: impl FnMut(
        &[u8],
        GovernanceCatalogReadLimits,
    ) -> Result<Option<LoadedGovernanceRecord<T>>, StateTransactionError>,
    compare: impl Fn(&T, &T) -> Ordering,
    limits: GovernanceCatalogCollectionLimits,
) -> Result<GovernanceCatalogCollection<T>, GovernanceCatalogCollectionError> {
    use GovernanceCatalogCollectionError as E;
    if keys.iter().any(|key| key.len() > 65535) {
        return Err(E::InvalidOptions);
    }
    let mut result = rank::start(limit, limits)?;
    for key in keys {
        if let Some(candidate) = loader(key, limits.load).map_err(E::Source)? {
            rank::retain(&mut result, candidate, limit, &compare, limits)?;
        }
    }
    Ok(result)
}

fn valid_page(
    page: &GovernanceCatalogPage,
    cursor: Option<&[u8]>,
    limits: GovernanceCatalogCollectionLimits,
) -> Result<(), GovernanceCatalogCollectionError> {
    use GovernanceCatalogCollectionError as E;
    if page.members.len() > 256
        || page.full != (page.members.len() == 256)
        || !page.members.windows(2).all(|pair| pair[0] < pair[1])
        || page
            .members
            .first()
            .is_some_and(|key| cursor.is_some_and(|cursor| key.as_slice() <= cursor))
        || page.members.iter().any(|key| key.len() > 65535)
    {
        return Err(E::InvalidPage);
    }
    let bytes = page
        .members
        .iter()
        .try_fold(128 + 256 * std::mem::size_of::<Vec<u8>>(), |n, member| {
            n.checked_add(member.len())
        })
        .ok_or(E::Limit)?;
    if bytes > limits.page.maximum_bytes {
        return Err(E::Limit);
    }
    Ok(())
}
