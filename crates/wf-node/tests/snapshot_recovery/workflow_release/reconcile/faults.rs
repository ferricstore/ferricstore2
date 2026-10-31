//! Confirmed progress and uncertain original credit ownership scenarios.
use super::{
    fixtures::{ReleaseReconcileFault as Fault, ReleaseReconcileTrace as Trace},
    *,
};
use wf_ports::state::{StateTransactionError as S, WorkflowReleaseReadPort};

#[test]
fn reconciler_unknown_credit_preserves_original_context_and_never_acknowledges_its_intent()
-> TestResult {
    use wf_application::governance::{PublicCreditCallResult, retry_public_credit_call};
    let mut port = Fixture::new()?;
    let sources = port.sources.clone();
    port.fault = Fault::PublicUnknown;
    let mut first = run(
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
        (first.released, first.errors, first.pending.len()),
        (0, 1, 1)
    );
    assert!(
        !port
            .trace
            .iter()
            .any(|t| matches!(t, Trace::Ack(..) | Trace::Mark(_)))
    );
    let (log, state) = port.open(&sources[0])?;
    let queue = state.read_workflow_release_outbox(&sources[0], None, 256, 10000, 16 * f::MIB)?;
    assert_eq!(queue.entries.len(), 1);
    assert!(!queue.entries[0].completed);
    drop((log, state));
    port.receive_snapshot()?;
    let original = first.pending.pop().ok_or("original unknown call")?;
    let bytes = original.frame().to_vec();
    let result = retry_public_credit_call(original, &mut port, Limits::default().credit)?;
    assert!(matches!(result, PublicCreditCallResult::Retained(_)));
    assert_eq!(port.frames.last().ok_or("retry")?.as_bytes(), bytes);
    assert!(std::sync::Arc::ptr_eq(
        port.frames.first().ok_or("first")?,
        port.frames.last().ok_or("retry")?
    ));
    let second = run(
        Request {
            sources: &sources,
            cursor: first.next_cursor.as_deref(),
            reservation_limit: 256,
            now_ms: 106,
        },
        &mut port,
        Limits::default(),
    )?;
    assert_eq!((second.released, second.errors), (1, 0));
    port.assert_released()?;
    port.cleanup()
}

#[test]
fn reconciler_lost_ack_reply_reloads_actual_queue_without_another_credit_release() -> TestResult {
    let mut port = Fixture::new()?;
    let sources = port.sources.clone();
    port.fault = Fault::AckUnknown;
    let first = run(
        Request {
            sources: &sources,
            cursor: None,
            reservation_limit: 256,
            now_ms: 106,
        },
        &mut port,
        Limits::default(),
    )?;
    assert_eq!((first.released, first.errors), (1, 1));
    port.assert_released()?;
    let calls = port
        .trace
        .iter()
        .filter(|t| matches!(t, Trace::Credit(_)))
        .count();
    let second = run(
        Request {
            sources: &sources,
            cursor: first.next_cursor.as_deref(),
            reservation_limit: 256,
            now_ms: 106,
        },
        &mut port,
        Limits::default(),
    )?;
    assert_eq!(
        (second.released, second.errors, second.next_cursor),
        (0, 0, None)
    );
    assert_eq!(
        port.trace
            .iter()
            .filter(|t| matches!(t, Trace::Credit(_)))
            .count(),
        calls
    );
    port.cleanup()
}

#[test]
fn reconciler_unreadable_corrupt_or_wrong_cardinality_records_never_become_missing() -> TestResult {
    for fault in [
        Fault::RecordRead,
        Fault::RecordCardinality,
        Fault::RecordCorrupt,
        Fault::Route,
        Fault::PublicRefuse,
        Fault::OldStamp,
    ] {
        let mut port = Fixture::new()?;
        let sources = port.sources.clone();
        if fault == Fault::PublicRefuse {
            wf_application::governance::repair_credit_catalog(
                &mut port,
                &sources[0],
                &sources[0],
                b"limit",
                b"global",
                Limits::default().credit.publication,
            )?;
        }
        port.fault = fault;
        let before = p::cohort(&port.path(&sources[0])?)?;
        let logs = f::log_rows(&port.path(&sources[0])?)?;
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
            (result.released, result.errors, result.read_batches),
            (0, 1, 1),
            "{fault:?}: {:?}",
            port.last_error
        );
        assert_eq!(p::cohort(&port.path(&sources[0])?)?, before);
        assert_eq!(f::log_rows(&port.path(&sources[0])?)?, logs);
        port.cleanup()?;
    }
    Ok(())
}

#[test]
fn native_local_record_observation_does_not_opt_into_release_authorizing_freshness() -> TestResult {
    let port = Fixture::new()?;
    let s = &port.sources[0];
    let (log, mut state) = port.open(s)?;
    let read = Limits::default().read;
    assert_eq!(
        state.workflow_release_records_for_release(s, Some(b"p"), &[b"i"], read),
        Err(S::UnsupportedPolicyProfile)
    );
    for partition in [None, Some(b"".as_slice())] {
        assert_eq!(
            state.read_workflow_release_records(s, partition, &[b"i"], read),
            Err(S::UnsupportedPolicyProfile)
        );
    }
    assert!(matches!(
        state
            .read_workflow_release_records(s, Some(b"p"), &[b"i"], read)?
            .first(),
        Some(wf_ports::state::WorkflowReleaseRecord::Record { .. })
    ));
    drop((log, state));
    port.cleanup()
}

#[test]
fn reconciler_global_invalid_options_and_cursor_refuse_before_port_calls() -> TestResult {
    let mut port = Fixture::new()?;
    let sources = port.sources.clone();
    for cursor in [Some(b"v2:0|".as_slice()), Some(b"v1:0|0=1,0=2")] {
        assert!(
            run(
                Request {
                    sources: &sources,
                    cursor,
                    reservation_limit: 256,
                    now_ms: 106
                },
                &mut port,
                Limits::default()
            )
            .is_err()
        );
    }
    for reservation_limit in [0, 257] {
        assert!(
            run(
                Request {
                    sources: &sources,
                    cursor: None,
                    reservation_limit,
                    now_ms: 106
                },
                &mut port,
                Limits::default()
            )
            .is_err()
        );
    }
    let limits = Limits {
        workspace_bytes: 1,
        ..Limits::default()
    };
    assert!(
        run(
            Request {
                sources: &sources,
                cursor: None,
                reservation_limit: 256,
                now_ms: 106
            },
            &mut port,
            limits
        )
        .is_err()
    );
    assert_eq!(port.trace, Vec::<Trace>::new());
    port.cleanup()
}
