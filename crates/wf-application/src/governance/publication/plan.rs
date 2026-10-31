//! Neutral prospective bounds and complete observations, without paid authority.

use super::{
    CreditCatalogPublicationError as E, CreditCatalogPublicationLimits as Limits,
    CreditCatalogPublicationRequest as Request,
};
use wf_domain::workflow::{MAX_EXACT_MILLIS as MAX, request::RequestScope};
use wf_ports::state::{CreditCatalogEntry, CreditCatalogOwnerState, CreditCatalogPage};

pub(in crate::governance) fn select(
    request: Request<'_>,
    limits: Limits,
) -> Result<(&RequestScope, u64), E> {
    if request.sources.is_empty()
        || request.source_cursor > MAX
        || request.catalog.len() > 65535
        || !(1..=256).contains(&limits.page_size)
        || [
            limits.read.maximum_steps,
            limits.read.maximum_bytes,
            limits.read.maximum_temporary_bytes,
            limits.maximum_members_bytes,
            limits.execution_workspace_bytes,
            limits.workspace_bytes,
        ]
        .contains(&0)
    {
        return Err(E::InvalidOptions);
    }
    let count = u64::try_from(request.sources.len()).map_err(|_| E::InvalidOptions)?;
    if count > MAX {
        return Err(E::InvalidOptions);
    }
    let selected = request.source_cursor % count;
    let source = &request.sources[usize::try_from(selected).map_err(|_| E::InvalidOptions)?];
    if source.tenant() != request.target.tenant() {
        return Err(E::InvalidOptions);
    }
    let reserved = [
        limits.read.maximum_bytes,
        limits.read.maximum_bytes,
        limits.read.maximum_temporary_bytes,
        limits.maximum_members_bytes,
        16_384,
        limits.codec.admission().decision_bytes(),
        2048,
        limits.execution_workspace_bytes,
    ]
    .into_iter()
    .try_fold(0_usize, usize::checked_add)
    .ok_or(E::Limit)?;
    if reserved > limits.workspace_bytes {
        return Err(E::Limit);
    }
    Ok((source, (selected + 1) % count))
}

pub(super) fn valid_page(page: &CreditCatalogPage, limits: Limits) -> bool {
    let pending = page.meta.tail() + 1 - page.meta.head();
    let expected = pending.min(u64::from(limits.page_size));
    if pending > 65_536
        || u64::try_from(page.entries.len()).ok() != Some(expected)
        || page.more != (expected < pending)
    {
        return false;
    }
    let Some(mut owned) = page
        .entries
        .len()
        .checked_mul(std::mem::size_of::<CreditCatalogEntry>())
        .and_then(|n| n.checked_add(128))
    else {
        return false;
    };
    for (index, entry) in page.entries.iter().enumerate() {
        let Ok(offset) = u64::try_from(index) else {
            return false;
        };
        if entry.sequence != page.meta.head() + offset
            || entry.credit_scope.is_empty()
            || entry.credit_scope.len() > 65535
        {
            return false;
        }
        let Some(next) = owned.checked_add(entry.credit_scope.len()) else {
            return false;
        };
        owned = next;
    }
    owned <= limits.read.maximum_bytes
}

pub(super) fn valid_owners(
    owners: &[CreditCatalogOwnerState],
    expected: usize,
    limits: Limits,
) -> bool {
    owners.len() == expected
        && owners
            .len()
            .checked_mul(std::mem::size_of::<CreditCatalogOwnerState>())
            .and_then(|n| n.checked_add(128))
            .is_some_and(|n| n <= limits.read.maximum_bytes)
}
