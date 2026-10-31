//! Prospective whole-call ownership and complete original page validation.
use super::{
    ReleaseReconcileError as E, ReleaseReconcileLimits as L, ReleaseReconcileRequest as R,
    ReleaseReconcileStats,
};
use wf_domain::{
    governance::release::{ReleaseReconcileCursor, validate_release_state},
    workflow::MAX_EXACT_MILLIS as MAX,
};
use wf_ports::state::WorkflowReleasePage;

pub(super) fn start(
    request: R<'_>,
    limits: L,
) -> Result<(ReleaseReconcileCursor, ReleaseReconcileStats, usize), E> {
    if request.sources.is_empty()
        || request.sources.len() > limits.maximum_sources
        || request.sources.len() as u128 > u128::from(MAX)
        || !(1..=256).contains(&request.reservation_limit)
        || request.now_ms > MAX
        || [
            limits.read.maximum_steps,
            limits.read.maximum_bytes,
            limits.read.maximum_temporary_bytes,
            limits.maximum_cursor_bytes,
            limits.maximum_maintenance_bytes,
            limits.execution_workspace_bytes,
            limits.workspace_bytes,
            limits.credit.workspace_bytes,
            limits.credit.maximum_prepared_bytes,
            limits.credit.maximum_response_bytes,
            limits.credit.maximum_ids_bytes,
            limits.credit.publication.read.maximum_steps,
            limits.credit.publication.read.maximum_bytes,
            limits.credit.publication.read.maximum_temporary_bytes,
            limits.credit.publication.workspace_bytes,
            limits.credit.publication.execution_workspace_bytes,
        ]
        .contains(&0)
    {
        return Err(E::InvalidOptions);
    }
    let base = reserved(request, limits)?;
    if base > limits.workspace_bytes {
        return Err(E::Limit);
    }
    let cursor = ReleaseReconcileCursor::decode(
        request.cursor,
        u64::try_from(request.sources.len()).map_err(|_| E::Limit)?,
        limits.maximum_cursor_bytes,
    )
    .map_err(E::Cursor)?;
    let mut configured = Vec::new();
    configured
        .try_reserve_exact(request.sources.len())
        .map_err(|_| E::Allocation)?;
    configured.extend(request.sources);
    configured.sort_unstable_by_key(|s| {
        (
            s.tenant().as_bytes(),
            s.group().as_bytes(),
            s.recovery().as_bytes(),
        )
    });
    if configured.windows(2).any(|p| p[0] == p[1]) {
        return Err(E::InvalidOptions);
    }
    let mut stats = ReleaseReconcileStats::default();
    stats
        .pending
        .try_reserve_exact(usize::from(request.reservation_limit))
        .map_err(|_| E::Allocation)?;
    Ok((cursor, stats, base))
}
pub(super) fn admit(base: usize, stats: &ReleaseReconcileStats, limits: L) -> bool {
    stats
        .pending
        .iter()
        .try_fold(base, |n, p| n.checked_add(p.ownership_bytes()?))
        .is_some_and(|n| n <= limits.workspace_bytes)
}
pub(super) fn valid(page: &WorkflowReleasePage, start: Option<u64>, limit: u16, limits: L) -> bool {
    if validate_release_state(page.meta).is_err() || page.entries.len() > usize::from(limit) {
        return false;
    }
    let first = start
        .filter(|n| *n >= page.meta.head && *n <= page.meta.tail)
        .unwrap_or(page.meta.head);
    let count = (page.meta.tail + 1)
        .saturating_sub(first)
        .min(u64::from(limit));
    if page.entries.len() as u128 != u128::from(count)
        || page.more != (first + count <= page.meta.tail)
    {
        return false;
    }
    let mut bytes = size_of::<WorkflowReleasePage>();
    for (at, entry) in page.entries.iter().enumerate() {
        if u64::try_from(at).ok().and_then(|n| first.checked_add(n)) != Some(entry.sequence)
            || entry.intent().is_err()
        {
            return false;
        }
        let Some(next) = bytes
            .checked_add(size_of::<wf_ports::state::WorkflowReleaseEntry>())
            .and_then(|n| n.checked_add(entry.encoded().len()))
        else {
            return false;
        };
        bytes = next;
    }
    bytes <= limits.read.maximum_bytes
}

fn reserved(request: R<'_>, limits: L) -> Result<usize, E> {
    [
        request
            .sources
            .iter()
            .try_fold(65536_usize, |n, s| {
                n.checked_add(size_of::<wf_domain::workflow::request::RequestScope>())?
                    .checked_add(size_of::<usize>())?
                    .checked_add(s.tenant().as_bytes().len())?
                    .checked_add(s.group().as_bytes().len())?
                    .checked_add(s.recovery().as_bytes().len())
            })
            .ok_or(E::Limit)?,
        limits.maximum_cursor_bytes,
        limits.maximum_cursor_bytes,
        limits.maximum_cursor_bytes,
        limits.maximum_cursor_bytes,
        limits.read.maximum_bytes,
        limits.read.maximum_bytes,
        limits.read.maximum_temporary_bytes,
        limits.maximum_maintenance_bytes,
        limits.execution_workspace_bytes,
        limits.credit.workspace_bytes,
    ]
    .into_iter()
    .try_fold(0_usize, usize::checked_add)
    .ok_or(E::Limit)
}
