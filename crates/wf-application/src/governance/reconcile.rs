//! Bounded automatic release orchestration over configured semantic capabilities.
mod dispatch;
mod model;
mod observe;
mod plan;
mod progress;
pub use model::{
    ReleaseReconcileError, ReleaseReconcileLimits, ReleaseReconcileRequest, ReleaseReconcileStats,
};
use wf_ports::consensus::{PublicCreditSubmitPort, WorkflowReleaseSubmitPort};
/// Reconciles at most one nonempty page, rotating fairly after known progress.
/// # Errors
/// Refuses invalid configured options, cursor or prospective ownership before work.
pub fn run_workflow_release_reconcile_once<P>(
    request: ReleaseReconcileRequest<'_>,
    port: &mut P,
    limits: ReleaseReconcileLimits,
) -> Result<ReleaseReconcileStats, ReleaseReconcileError>
where
    P: PublicCreditSubmitPort + WorkflowReleaseSubmitPort,
{
    use model::ReleaseStatus as S;
    let (mut cursor, mut stats, base) = plan::start(request, limits)?;
    let count = request.sources.len();
    let first = usize::try_from(cursor.next_shard()).map_err(|_| ReleaseReconcileError::Limit)?;
    for offset in 0..count {
        let index = (first + offset) % count;
        let shard = u64::try_from(index).map_err(|_| ReleaseReconcileError::Limit)?;
        let start = cursor.position(shard);
        let scope = &request.sources[index];
        let page = match port.workflow_release_page(
            scope,
            start,
            request.reservation_limit,
            limits.read,
        ) {
            Ok(page) if plan::valid(&page, start, request.reservation_limit, limits) => page,
            _ => {
                stats.errors += 1;
                continue;
            }
        };
        if page.entries.is_empty() {
            continue;
        }
        // Allocate both possible cursor owners before any release can be submitted.
        let next = page
            .entries
            .last()
            .filter(|e| e.sequence < page.meta.tail)
            .map(|e| e.sequence + 1);
        cursor
            .advance(shard, next)
            .map_err(ReleaseReconcileError::Cursor)?;
        let success_cursor = cursor
            .encode(limits.maximum_cursor_bytes)
            .map_err(ReleaseReconcileError::Cursor)?;
        cursor
            .advance(shard, None)
            .map_err(ReleaseReconcileError::Cursor)?;
        let retry_cursor = cursor
            .encode(limits.maximum_cursor_bytes)
            .map_err(ReleaseReconcileError::Cursor)?;
        let mut intents = Vec::new();
        intents
            .try_reserve_exact(page.entries.len())
            .map_err(|_| ReleaseReconcileError::Allocation)?;
        for entry in &page.entries {
            intents.push(
                entry
                    .intent()
                    .map_err(|_| ReleaseReconcileError::InvalidOptions)?,
            );
        }
        let mut decisions = [S::Error; 256];
        stats.read_batches = observe::records(port, scope, &page, &intents, &mut decisions, limits);
        dispatch::release(
            port,
            scope,
            &intents,
            &mut decisions,
            (&mut stats, base, request.now_ms, limits),
        );
        for state in &decisions[..page.entries.len()] {
            match state {
                S::Released => stats.released += 1,
                S::Retained => stats.retained += 1,
                S::Error | S::Release => stats.errors += 1,
                S::Completed => (),
            }
        }
        let confirmed = progress::persist(port, scope, &page, &decisions, request.now_ms, limits);
        if !confirmed {
            stats.errors += 1;
        }
        drop(intents);
        drop(page);
        stats.next_cursor = Some(if confirmed {
            success_cursor
        } else {
            retry_cursor
        });
        return Ok(stats);
    }
    if stats.errors != 0 {
        let shard = cursor.next_shard();
        cursor
            .advance(shard, cursor.position(shard))
            .map_err(ReleaseReconcileError::Cursor)?;
        stats.next_cursor = Some(
            cursor
                .encode(limits.maximum_cursor_bytes)
                .map_err(ReleaseReconcileError::Cursor)?,
        );
    }
    Ok(stats)
}
