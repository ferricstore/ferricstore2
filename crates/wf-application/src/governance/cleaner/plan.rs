//! Prospective cohort and configured routing checks, never cleanup authority.
use super::{
    CreditCleanupError as E, CreditCleanupFailure as F, CreditCleanupLimits as L,
    CreditCleanupRequest as R, CreditCleanupSource,
};
use wf_domain::workflow::MAX_EXACT_MILLIS as MAX;
use wf_ports::state::{
    GovernanceCatalogPage, GovernanceCatalogReadLimits, decode_credit_catalog_member,
};

pub(super) fn preflight(request: R<'_>, budget: Option<u16>, limits: L) -> Result<u16, E> {
    let budget = budget.unwrap_or(16);
    if !(1..=64).contains(&budget)
        || request.now_ms > MAX
        || request.catalog.len() > 65535
        || request.sources.is_empty()
        || request.sources.len() > limits.maximum_sources
        || request.target.tenant().as_bytes().len() > 256
        || [
            limits.maximum_sources,
            limits.read.maximum_steps,
            limits.read.maximum_bytes,
            limits.read.maximum_temporary_bytes,
            limits.execution_workspace_bytes,
            limits.workspace_bytes,
        ]
        .contains(&0)
        || request.sources.iter().any(|source| {
            source.shard_count == 0
                || source.shard_count > MAX
                || source.scope.tenant() != request.target.tenant()
        })
    {
        return Err(E::InvalidOptions);
    }
    let metadata = request
        .sources
        .len()
        .checked_mul(std::mem::size_of::<CreditCleanupSource<'_>>())
        .ok_or(E::Limit)?;
    let reserved = [
        metadata,
        limits.read.maximum_bytes,
        limits.read.maximum_bytes,
        limits.read.maximum_temporary_bytes,
        limits.codec.admission().decision_bytes(),
        limits.codec.admission().decision_bytes(),
        limits.execution_workspace_bytes,
        4 * 65535,
        18432,
        4096,
    ]
    .into_iter()
    .try_fold(0_usize, usize::checked_add)
    .ok_or(E::Limit)?;
    if reserved > limits.workspace_bytes {
        return Err(E::Limit);
    }
    Ok(budget)
}
pub(super) fn read(limits: L) -> GovernanceCatalogReadLimits {
    GovernanceCatalogReadLimits {
        maximum_steps: limits.read.maximum_steps,
        maximum_bytes: limits.read.maximum_bytes,
        maximum_temporary_bytes: limits.read.maximum_temporary_bytes,
    }
}
pub(super) fn valid_page(page: &GovernanceCatalogPage, cursor: Option<&[u8]>, limits: L) -> bool {
    if page.members.len() > 1 || page.full != (page.members.len() == 1) {
        return false;
    }
    let owned = page.members.iter().try_fold(128_usize, |n, member| {
        n.checked_add(std::mem::size_of::<Vec<u8>>())?
            .checked_add(member.len())
    });
    owned.is_some_and(|n| n <= limits.read.maximum_bytes)
        && page.members.first().is_none_or(|member| {
            member.len() <= 65535 && cursor.is_none_or(|cursor| member.as_slice() > cursor)
        })
}
pub(super) fn source<'a, 'm>(
    request: R<'a>,
    member: &'m [u8],
) -> Result<(CreditCleanupSource<'a>, &'m [u8]), F> {
    let identity =
        decode_credit_catalog_member(member, 65535).map_err(|_| F::InvalidObservation)?;
    let mut matches = request.sources.iter().filter(|source| {
        source.scope.group().as_bytes() == identity.group
            && source.scope.recovery().as_bytes() == identity.recovery
    });
    let source = *matches.next().ok_or(F::InvalidObservation)?;
    if matches.next().is_some() {
        return Err(F::InvalidObservation);
    }
    Ok((source, identity.credit_scope))
}
