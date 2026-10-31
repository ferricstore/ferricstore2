//! Automatic orchestration over actual serial committed Node databases.
use super::{TestResult, f, fixtures as p, frames as q};
use wf_application::governance::{
    ReleaseReconcileLimits as Limits, ReleaseReconcileRequest as Request,
    run_workflow_release_reconcile_once as run,
};
mod credit;
mod fairness;
mod faults;
mod fixtures;
mod ports;
use fixtures::ReleaseReconcileFixture as Fixture;

#[test]
fn reconciler_automatically_releases_actual_paid_terminal_original_and_preserves_running_sibling()
-> TestResult {
    let mut port = Fixture::new()?;
    let sources = port.sources.clone();
    let result = run(
        Request {
            sources: &sources,
            cursor: None,
            reservation_limit: 256,
            now_ms: 106,
        },
        &mut port,
        Limits::default(),
    )?;
    assert_eq!(
        (
            result.released,
            result.retained,
            result.errors,
            result.read_batches
        ),
        (1, 0, 0, 1)
    );
    assert!(result.pending.is_empty());
    port.assert_released()?;
    let second = run(
        Request {
            sources: &sources,
            cursor: result.next_cursor.as_deref(),
            reservation_limit: 256,
            now_ms: 107,
        },
        &mut port,
        Limits::default(),
    )?;
    assert_eq!(
        (second.released, second.errors, second.next_cursor),
        (0, 0, None)
    );
    port.cleanup()
}
