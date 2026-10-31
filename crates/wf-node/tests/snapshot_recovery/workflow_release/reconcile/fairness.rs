//! Source-style seeded holes and fair configured source traversal scenarios.
use super::{
    fixtures::{ReleaseReconcileFault as Fault, ReleaseReconcileTrace as Trace},
    *,
};
use wf_domain::governance::release::ReleaseIntentRef;

#[test]
fn reconciler_actual_corrupt_queue_cannot_starve_healthy_later_source_or_mutate_failed_cohort()
-> TestResult {
    let mut port = Fixture::running()?;
    let later = port.add_source()?;
    let intent = ReleaseIntentRef {
        flow_id: b"missing",
        partition: Some(b"p"),
        scope: b"global",
        shard_id: 0,
        reservation_id: b"a\0",
        enforcement: wf_domain::governance::release::LimitEnforcement::StrictGlobal,
        created_at: wf_types::version::TimestampMillis::new(104),
    };
    port.seed_queue(0, &[intent], &[])?;
    port.seed_queue(later, &[intent], &[])?;
    port.corrupt_first_intent(0)?;
    let sources = port.sources.clone();
    assert!(port.open(&sources[0]).is_err());
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
    assert_eq!((result.released, result.errors), (1, 1));
    assert_eq!(p::cohort(&port.path(&sources[0])?)?, before);
    assert_eq!(f::log_rows(&port.path(&sources[0])?)?, logs);
    let (log, state) = port.open(&sources[later])?;
    assert_eq!(
        state
            .read_credit_owner(
                &sources[later],
                b"global",
                wf_domain::governance::credit::CreditLimits::default(),
                10000,
                16 * f::MIB
            )?
            .ok_or("healthy owner")?
            .view()
            .leases[0]
            .in_use,
        1
    );
    drop((log, state));
    port.cleanup()
}

#[test]
fn reconciler_completed_markers_acknowledge_without_record_reads_or_credit_resubmission()
-> TestResult {
    use wf_domain::governance::release::ReleaseMaintenanceRequest;
    let mut port = Fixture::new()?;
    let sources = port.sources.clone();
    let wire = port.terminal_intent()?;
    let intent = wf_ports::log::inspect_release_intent(&wire, wire.len())?;
    port.seed_queue(0, &vec![intent; 256], &[])?;
    let frames = q::all()?;
    port.commit(
        &sources[0],
        &wf_types::identity::RequestId::new(b"release".to_vec())?,
        &frames[6],
    )?;
    let command = ReleaseMaintenanceRequest::MarkCompleted {
        sequences: (1..=256).collect(),
    };
    let mark = q::maintenance(b"precompleted", 107, &command)?;
    port.commit(
        &sources[0],
        &wf_types::identity::RequestId::new(b"precompleted".to_vec())?,
        &mark,
    )?;
    port.now = 107;
    let result = run(
        Request {
            sources: &sources,
            cursor: None,
            reservation_limit: 256,
            now_ms: 107,
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
        (0, 0, 0, 0)
    );
    assert!(
        !port
            .trace
            .iter()
            .any(|t| matches!(t, Trace::Records(..) | Trace::Credit(_)))
    );
    assert!(port.trace.contains(&Trace::Ack(1, 256)));
    port.assert_released()?;
    port.cleanup()
}

#[test]
fn reconciler_one_page_bound_batches_256_duplicates_and_resumes_at_257() -> TestResult {
    let mut port = Fixture::new()?;
    let sources = port.sources.clone();
    let wire = port.terminal_intent()?;
    let intent = wf_ports::log::inspect_release_intent(&wire, wire.len())?;
    port.seed_queue(0, &vec![intent; 257], &[])?;
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
    assert_eq!(
        (first.released, first.errors, first.read_batches),
        (256, 0, 1)
    );
    assert_eq!(first.next_cursor.as_deref(), Some(b"v1:0|0=257".as_slice()));
    assert!(
        port.trace
            .contains(&Trace::Records(Some(b"p".to_vec()), 256))
    );
    assert!(port.trace.contains(&Trace::Ack(1, 256)));
    let (log, state) = port.open(&sources[0])?;
    let page = state.read_workflow_release_outbox(&sources[0], None, 256, 10000, 16 * f::MIB)?;
    assert_eq!(
        (page.meta.head, page.meta.tail, page.entries.len()),
        (257, 257, 1)
    );
    drop((log, state));
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
    assert!(port.trace.contains(&Trace::Ack(257, 257)));
    port.assert_released()?;
    port.cleanup()
}

#[test]
fn reconciler_group_dedup_keeps_first_original_order_without_releasing_running_sibling()
-> TestResult {
    let mut port = Fixture::new()?;
    port.reserve(b"c")?;
    let sources = port.sources.clone();
    let wire = port.terminal_intent()?;
    let terminal = wf_ports::log::inspect_release_intent(&wire, wire.len())?;
    let missing = ReleaseIntentRef {
        flow_id: b"missing",
        reservation_id: b"c",
        ..terminal
    };
    port.seed_queue(0, &[missing, terminal, missing, terminal], &[])?;
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
        (4, 0, 1)
    );
    assert!(
        port.trace
            .contains(&Trace::Credit(vec![b"c".to_vec(), b"a\0".to_vec()]))
    );
    port.assert_released()?;
    port.cleanup()
}

#[test]
fn reconciler_rotates_retained_head_and_skips_failed_source_without_starving_later_shards()
-> TestResult {
    for unavailable in [false, true] {
        let mut port = Fixture::running()?;
        let later = port.add_source()?;
        let held = ReleaseIntentRef {
            flow_id: b"j",
            partition: Some(b"p"),
            scope: b"global",
            shard_id: 0,
            reservation_id: b"b",
            enforcement: wf_domain::governance::release::LimitEnforcement::StrictGlobal,
            created_at: wf_types::version::TimestampMillis::new(104),
        };
        port.seed_queue(0, &[held], &[])?;
        port.seed_queue(
            later,
            &[ReleaseIntentRef {
                flow_id: b"missing",
                reservation_id: b"a\0",
                ..held
            }],
            &[],
        )?;
        let sources = port.sources.clone();
        if unavailable {
            port.fault = Fault::PageUnavailable(0);
        }
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
        if unavailable {
            assert_eq!((first.released, first.retained, first.errors), (1, 0, 1));
        } else {
            assert_eq!((first.released, first.retained, first.errors), (0, 1, 0));
            assert_eq!(first.next_cursor.as_deref(), Some(b"v1:1|".as_slice()));
            let next = run(
                Request {
                    sources: &sources,
                    cursor: first.next_cursor.as_deref(),
                    reservation_limit: 256,
                    now_ms: 106,
                },
                &mut port,
                Limits::default(),
            )?;
            assert_eq!((next.released, next.retained, next.errors), (1, 0, 0));
        }
        for (index, expected) in [(0, 2), (later, 1)] {
            let (log, state) = port.open(&sources[index])?;
            assert_eq!(
                state
                    .read_credit_owner(
                        &sources[index],
                        b"global",
                        wf_domain::governance::credit::CreditLimits::default(),
                        10000,
                        16 * f::MIB
                    )?
                    .ok_or("owner")?
                    .view()
                    .leases[0]
                    .in_use,
                expected
            );
            drop((log, state));
        }
        port.cleanup()?;
    }
    Ok(())
}

#[test]
fn reconciler_received_snapshot_keeps_real_original_queue_and_automatically_releases_it()
-> TestResult {
    let mut port = Fixture::new()?;
    port.receive_snapshot()?;
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
    port.assert_released()?;
    port.cleanup()
}

#[test]
fn reconciler_rejects_wrong_original_maintenance_receipt_before_cursor_advance() -> TestResult {
    let mut port = Fixture::new()?;
    let sources = port.sources.clone();
    let wire = port.terminal_intent()?;
    let terminal = wf_ports::log::inspect_release_intent(&wire, wire.len())?;
    let held = ReleaseIntentRef {
        flow_id: b"j",
        reservation_id: b"b",
        ..terminal
    };
    port.seed_queue(0, &[held, terminal, terminal], &[])?;
    port.fault = Fault::WrongMaintenanceReply;
    let result = run(
        Request {
            sources: &sources,
            cursor: None,
            reservation_limit: 2,
            now_ms: 106,
        },
        &mut port,
        Limits::default(),
    )?;
    assert_eq!((result.released, result.retained, result.errors), (1, 1, 1));
    assert_eq!(result.next_cursor.as_deref(), Some(b"v1:0|".as_slice()));
    port.cleanup()
}

#[test]
fn reconciler_duplicate_intents_release_unique_original_ids_and_acknowledge_every_replay()
-> TestResult {
    let mut port = Fixture::new()?;
    let sources = port.sources.clone();
    let wire = port.terminal_intent()?;
    let intent = wf_ports::log::inspect_release_intent(&wire, wire.len())?;
    port.seed_queue(0, &[intent, intent], &[])?;
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
        (2, 0, 1)
    );
    let calls: Vec<_> = port
        .trace
        .iter()
        .filter_map(|t| {
            if let Trace::Credit(ids) = t {
                Some(ids)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(calls, vec![&vec![b"a\0".to_vec()]]);
    assert!(port.trace.contains(&Trace::Ack(1, 2)));
    port.assert_released()?;
    port.cleanup()
}

#[test]
fn reconciler_sparse_completion_keeps_retained_head_and_resumes_after_one_page() -> TestResult {
    let mut port = Fixture::new()?;
    let sources = port.sources.clone();
    let wire = port.terminal_intent()?;
    let terminal = wf_ports::log::inspect_release_intent(&wire, wire.len())?;
    let held = ReleaseIntentRef {
        flow_id: b"j",
        reservation_id: b"b",
        ..terminal
    };
    port.seed_queue(0, &[held, terminal, terminal], &[])?;
    let first = run(
        Request {
            sources: &sources,
            cursor: None,
            reservation_limit: 2,
            now_ms: 106,
        },
        &mut port,
        Limits::default(),
    )?;
    assert_eq!((first.released, first.retained, first.errors), (1, 1, 0));
    assert_eq!(first.next_cursor.as_deref(), Some(b"v1:0|0=3".as_slice()));
    assert!(port.trace.contains(&Trace::Mark(vec![2])));
    assert!(!port.trace.iter().any(|t| matches!(t, Trace::Ack(..))));
    let second = run(
        Request {
            sources: &sources,
            cursor: first.next_cursor.as_deref(),
            reservation_limit: 2,
            now_ms: 106,
        },
        &mut port,
        Limits::default(),
    )?;
    assert_eq!((second.released, second.retained, second.errors), (1, 0, 0));
    let third = run(
        Request {
            sources: &sources,
            cursor: second.next_cursor.as_deref(),
            reservation_limit: 2,
            now_ms: 106,
        },
        &mut port,
        Limits::default(),
    )?;
    assert_eq!(
        (
            third.released,
            third.retained,
            third.errors,
            third.read_batches
        ),
        (0, 1, 0, 1)
    );
    let (log, state) = port.open(&sources[0])?;
    let page = state.read_workflow_release_outbox(&sources[0], None, 256, 10000, 16 * f::MIB)?;
    assert_eq!(
        (page.meta.head, page.meta.tail, page.entries.len()),
        (1, 3, 3)
    );
    assert_eq!(
        page.entries.iter().map(|e| e.completed).collect::<Vec<_>>(),
        vec![false, true, true]
    );
    drop((log, state));
    port.cleanup()
}

#[test]
fn reconciler_lost_sparse_mark_reply_restarts_at_head_without_rereleasing_completed_entry()
-> TestResult {
    let mut port = Fixture::new()?;
    let sources = port.sources.clone();
    let wire = port.terminal_intent()?;
    let terminal = wf_ports::log::inspect_release_intent(&wire, wire.len())?;
    let held = ReleaseIntentRef {
        flow_id: b"j",
        reservation_id: b"b",
        ..terminal
    };
    port.seed_queue(0, &[held, terminal, terminal], &[])?;
    port.fault = Fault::MarkUnknown;
    let first = run(
        Request {
            sources: &sources,
            cursor: None,
            reservation_limit: 2,
            now_ms: 106,
        },
        &mut port,
        Limits::default(),
    )?;
    assert_eq!((first.released, first.retained, first.errors), (1, 1, 1));
    assert_eq!(first.next_cursor.as_deref(), Some(b"v1:0|".as_slice()));
    let calls = port
        .trace
        .iter()
        .filter(|t| matches!(t, Trace::Credit(_)))
        .count();
    let second = run(
        Request {
            sources: &sources,
            cursor: first.next_cursor.as_deref(),
            reservation_limit: 2,
            now_ms: 106,
        },
        &mut port,
        Limits::default(),
    )?;
    assert_eq!(
        (
            second.released,
            second.retained,
            second.errors,
            second.read_batches
        ),
        (0, 1, 0, 1)
    );
    assert_eq!(second.next_cursor.as_deref(), Some(b"v1:0|0=3".as_slice()));
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
fn reconciler_keeps_original_nil_and_present_empty_partitions_distinct_and_unavailable()
-> TestResult {
    let mut port = Fixture::new()?;
    let sources = port.sources.clone();
    let wire = port.terminal_intent()?;
    let terminal = wf_ports::log::inspect_release_intent(&wire, wire.len())?;
    port.seed_queue(
        0,
        &[
            ReleaseIntentRef {
                partition: None,
                ..terminal
            },
            ReleaseIntentRef {
                partition: Some(b""),
                ..terminal
            },
        ],
        &[],
    )?;
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
        (0, 2, 2)
    );
    assert!(port.trace.contains(&Trace::Records(None, 1)));
    assert!(port.trace.contains(&Trace::Records(Some(vec![]), 1)));
    assert!(
        !port
            .trace
            .iter()
            .any(|t| matches!(t, Trace::Credit(_) | Trace::Ack(..) | Trace::Mark(_)))
    );
    port.cleanup()
}
