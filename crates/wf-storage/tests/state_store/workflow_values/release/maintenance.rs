//! Exact bounded native completion and prefix acknowledgement on the real writer.
use super::*;
use wf_domain::{
    governance::release::{ReleaseMaintenanceReply as R, ReleaseMaintenanceRequest as C},
    workflow::request::{RetainedOutcome, RetainedStatus},
};
use wf_ports::log::{decode_release_maintenance_reply, encode_release_maintenance_request};
use wf_storage::transaction::{
    RecordBatchTransactionOutcome as O, apply_release_maintenance_transaction,
};
use wf_types::identity::RequestId;
mod custody;
mod flow;

fn apply(
    store: &mut RedbStateStore,
    request: &[u8],
    command: &C,
    index: u64,
) -> TestResult<Box<RetainedOutcome>> {
    let scope = crate::fixtures::scope()?;
    let encoded = encode_release_maintenance_request(
        &scope,
        &RequestId::new(request.to_vec())?,
        crate::fixtures::clock(100 + index)?,
        command,
        f::limits().record_batch.codec,
        f::limits().record_batch.workspace_bytes,
    )?;
    let result = apply_release_maintenance_transaction(
        store,
        &scope,
        encoded.as_bytes(),
        crate::fixtures::log(index),
        f::limits().record_batch,
    )?;
    let O::Retained { outcome, .. } = result else {
        return Err("maintenance receipt".into());
    };
    Ok(outcome)
}

fn reply(outcome: &RetainedOutcome) -> TestResult<R> {
    assert_eq!(outcome.intent().version(), 18);
    assert_eq!(outcome.response_version(), 15);
    Ok(decode_release_maintenance_reply(
        outcome.response(),
        25,
        1024,
    )?)
}

#[test]
fn release_completion_marks_original_sparse_cohort_and_ack_removes_exact_prefix() -> TestResult {
    let dir = crate::fixtures::directory("release-maintenance-complete-ack")?;
    let path = dir.join("state");
    fixture(&path)?;
    queue(&path)?;
    let mut store = open(&path)?;
    let completed = apply(
        &mut store,
        b"complete",
        &C::MarkCompleted {
            sequences: vec![2, 1],
        },
        1,
    )?;
    assert_eq!(completed.status(), RetainedStatus::Accepted);
    assert_eq!(
        reply(&completed)?,
        R::Accepted {
            meta: ReleaseOutboxState { head: 1, tail: 2 },
            affected: 2
        }
    );
    let scope = crate::fixtures::scope()?;
    let page = store.read_workflow_release_outbox(&scope, None, 256, 10000, 1024 * 1024)?;
    assert!(page.entries.iter().all(|entry| entry.completed));
    let acknowledged = apply(
        &mut store,
        b"ack",
        &C::Acknowledge {
            expected_head: 1,
            up_to: 1,
        },
        2,
    )?;
    assert_eq!(
        reply(&acknowledged)?,
        R::Accepted {
            meta: ReleaseOutboxState { head: 2, tail: 2 },
            affected: 1
        }
    );
    let page = store.read_workflow_release_outbox(&scope, None, 256, 10000, 1024 * 1024)?;
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].sequence, 2);
    assert!(page.entries[0].completed);
    let original = apply(
        &mut store,
        b"ack",
        &C::Acknowledge {
            expected_head: 1,
            up_to: 1,
        },
        3,
    )?;
    assert_eq!(acknowledged, original);
    let duplicate = apply(
        &mut store,
        b"ack-again",
        &C::Acknowledge {
            expected_head: 1,
            up_to: 1,
        },
        4,
    )?;
    assert_eq!(
        reply(&duplicate)?,
        R::Accepted {
            meta: ReleaseOutboxState { head: 2, tail: 2 },
            affected: 0
        }
    );
    drop(store);
    drop(open(&path)?);
    let rows = crate::fixtures::raw_rows(&path)?;
    assert!(!rows.contains_key(&key(22, Some(1), b"")?));
    assert!(!rows.contains_key(&key(23, Some(1), b"")?));
    assert!(rows.contains_key(&key(22, Some(2), b"")?));
    assert!(rows.contains_key(&key(23, Some(2), b"")?));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn release_ack_accepts_unmarked_prefix_and_sparse_completion_keeps_holes() -> TestResult {
    use wf_domain::governance::release::ReleaseMaintenanceRejection as J;
    let dir = crate::fixtures::directory("release-maintenance-holes")?;
    let path = dir.join("state");
    fixture(&path)?;
    queue(&path)?;
    let mut store = open(&path)?;
    // Source ACK is an explicit command: it does not require a completion marker.
    assert_eq!(
        reply(&*apply(
            &mut store,
            b"ack",
            &C::Acknowledge {
                expected_head: 1,
                up_to: 1
            },
            1
        )?)?,
        R::Accepted {
            meta: ReleaseOutboxState { head: 2, tail: 2 },
            affected: 1
        }
    );
    let stale = apply(
        &mut store,
        b"stale",
        &C::Acknowledge {
            expected_head: 1,
            up_to: 2,
        },
        2,
    )?;
    assert_eq!(
        reply(&stale)?,
        R::Rejected {
            meta: ReleaseOutboxState { head: 2, tail: 2 },
            reason: J::StaleCursor
        }
    );
    let refused = apply(
        &mut store,
        b"tail-overrun",
        &C::MarkCompleted {
            sequences: vec![2, 3],
        },
        3,
    )?;
    assert_eq!(
        reply(&refused)?,
        R::Rejected {
            meta: ReleaseOutboxState { head: 2, tail: 2 },
            reason: J::InvalidCompletion
        }
    );
    assert_eq!(
        reply(&*apply(
            &mut store,
            b"mark-old-and-pending",
            &C::MarkCompleted {
                sequences: vec![1, 2]
            },
            4
        )?)?,
        R::Accepted {
            meta: ReleaseOutboxState { head: 2, tail: 2 },
            affected: 1
        }
    );
    assert_completed_tail(&mut store, 2)?;
    apply(
        &mut store,
        b"drain",
        &C::Acknowledge {
            expected_head: 2,
            up_to: 2,
        },
        5,
    )?;
    assert_eq!(
        reply(&*apply(
            &mut store,
            b"old-prefix",
            &C::Acknowledge {
                expected_head: 1,
                up_to: 2
            },
            6
        )?)?,
        R::Accepted {
            meta: ReleaseOutboxState { head: 3, tail: 2 },
            affected: 0
        }
    );
    drop(store);
    drop(open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn release_ack_is_bounded_to_one_page_and_preserves_later_intent_and_marker() -> TestResult {
    let dir = crate::fixtures::directory("release-maintenance-page")?;
    let path = dir.join("state");
    fixture(&path)?;
    let mut rows = vec![(
        key(21, None, b"")?,
        encode_release_meta(ReleaseOutboxState { head: 1, tail: 257 }, 22)?,
    )];
    for sequence in 1..=257 {
        rows.push((key(22, Some(sequence), b"")?, intent(b"gone", b"paid")?));
    }
    rows.push((key(23, Some(257), b"")?, b"FWRC\0\x01".to_vec()));
    edit(&path, &rows, &[])?;
    let mut store = open(&path)?;
    assert_eq!(
        reply(&*apply(
            &mut store,
            b"page",
            &C::Acknowledge {
                expected_head: 1,
                up_to: 256
            },
            1
        )?)?,
        R::Accepted {
            meta: ReleaseOutboxState {
                head: 257,
                tail: 257
            },
            affected: 256
        }
    );
    let page = store.read_workflow_release_outbox(
        &crate::fixtures::scope()?,
        None,
        256,
        10000,
        1024 * 1024,
    )?;
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].sequence, 257);
    assert!(page.entries[0].completed);
    apply(
        &mut store,
        b"last",
        &C::Acknowledge {
            expected_head: 257,
            up_to: 257,
        },
        2,
    )?;
    drop(store);
    drop(open(&path)?);
    assert_eq!(
        crate::fixtures::raw_rows(&path)?
            .keys()
            .filter(|k| matches!(k.get(6), Some(22 | 23)))
            .count(),
        0
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn release_ack_preserves_exhausted_sequence_space_without_resetting_tail() -> TestResult {
    let dir = crate::fixtures::directory("release-maintenance-exhausted")?;
    let path = dir.join("state");
    fixture(&path)?;
    let max = wf_domain::workflow::MAX_EXACT_MILLIS;
    edit(
        &path,
        &[
            (
                key(21, None, b"")?,
                encode_release_meta(
                    ReleaseOutboxState {
                        head: max,
                        tail: max,
                    },
                    22,
                )?,
            ),
            (key(22, Some(max), b"")?, intent(b"gone", b"paid")?),
        ],
        &[],
    )?;
    let mut store = open(&path)?;
    assert_eq!(
        reply(&*apply(
            &mut store,
            b"last",
            &C::Acknowledge {
                expected_head: max,
                up_to: max
            },
            1
        )?)?,
        R::Accepted {
            meta: ReleaseOutboxState {
                head: max + 1,
                tail: max
            },
            affected: 1
        }
    );
    assert_eq!(
        reply(&*apply(
            &mut store,
            b"old",
            &C::MarkCompleted {
                sequences: vec![max]
            },
            2
        )?)?,
        R::Accepted {
            meta: ReleaseOutboxState {
                head: max + 1,
                tail: max
            },
            affected: 0
        }
    );
    drop(store);
    drop(open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn assert_completed_tail(store: &mut RedbStateStore, sequence: u64) -> TestResult {
    let page = store.read_workflow_release_outbox(
        &crate::fixtures::scope()?,
        None,
        256,
        10000,
        1024 * 1024,
    )?;
    assert_eq!(page.meta.head, sequence);
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].sequence, sequence);
    assert!(page.entries[0].completed);
    Ok(())
}
