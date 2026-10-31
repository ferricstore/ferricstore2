//! Pinned release queue, completion and read-status observations.
#![forbid(unsafe_code)]
mod cursor;
use wf_domain::{governance::release::*, workflow::MAX_EXACT_MILLIS as MAX};
use wf_types::version::TimestampMillis;
type TestResult = Result<(), Box<dyn std::error::Error>>;
#[test]
fn release_append_checks_complete_backlog_and_sequence_exhaustion() -> TestResult {
    let state = ReleaseOutboxState { head: 1, tail: 0 };
    let next = append_release_outbox(state, 65_536)?;
    assert_eq!((next.head, next.tail), (1, 65_536));
    assert_eq!(
        append_release_outbox(next, 1).err(),
        Some(ReleaseOutboxError::BacklogFull)
    );
    assert_eq!(
        append_release_outbox(state, 0).err(),
        Some(ReleaseOutboxError::Corrupt)
    );
    assert_eq!(
        append_release_outbox(
            ReleaseOutboxState {
                head: MAX + 1,
                tail: MAX
            },
            1
        )
        .err(),
        Some(ReleaseOutboxError::Corrupt)
    );
    assert!(
        validate_release_state(ReleaseOutboxState {
            head: MAX + 1,
            tail: MAX
        })
        .is_ok()
    );
    for state in [
        ReleaseOutboxState { head: 0, tail: 0 },
        ReleaseOutboxState { head: 2, tail: 0 },
        ReleaseOutboxState {
            head: 1,
            tail: MAX + 1,
        },
    ] {
        assert_eq!(
            validate_release_state(state),
            Err(ReleaseOutboxError::Corrupt)
        );
    }
    Ok(())
}
#[test]
fn release_acknowledgments_have_exact_page_bound_and_idempotent_error_order() -> TestResult {
    let state = ReleaseOutboxState { head: 1, tail: 257 };
    let plan = acknowledge_release_outbox(state, 1, 256)?;
    assert_eq!((plan.next().head, plan.next().tail), (257, 257));
    assert_eq!(
        plan.sequences().collect::<Vec<_>>(),
        (1..=256).collect::<Vec<_>>()
    );
    assert_eq!(
        acknowledge_release_outbox(state, 1, 257).err(),
        Some(ReleaseOutboxError::InvalidAcknowledgment)
    );
    let original = acknowledge_release_outbox(plan.next(), 1, 256)?;
    assert_eq!(original.sequences().count(), 0);
    assert_eq!((original.next().head, original.next().tail), (257, 257));
    assert_eq!(
        acknowledge_release_outbox(plan.next(), 1, 257).err(),
        Some(ReleaseOutboxError::StaleCursor)
    );
    assert_eq!(
        acknowledge_release_outbox(state, 1, 258).err(),
        Some(ReleaseOutboxError::InvalidAcknowledgment)
    );
    assert_eq!(
        acknowledge_release_outbox(state, 0, 1).err(),
        Some(ReleaseOutboxError::Corrupt)
    );
    let exhausted = acknowledge_release_outbox(
        ReleaseOutboxState {
            head: MAX,
            tail: MAX,
        },
        MAX,
        MAX,
    )?;
    assert_eq!(exhausted.next().head, MAX + 1);
    Ok(())
}
#[test]
fn release_completion_preserves_original_order_and_ignores_only_old_sequences() -> TestResult {
    let state = ReleaseOutboxState { head: 3, tail: 6 };
    let original = [6, 1, 4];
    let plan = prepare_release_completion(state, &original)?;
    assert_eq!(plan.sequences().collect::<Vec<_>>(), vec![6, 4]);
    for invalid in [&[][..], &[1, 1], &[0], &[7], &[2, 7]] {
        assert_eq!(
            prepare_release_completion(state, invalid).err(),
            Some(ReleaseOutboxError::InvalidCompletion)
        );
    }
    let too_many: Vec<_> = (1..=257).collect();
    assert_eq!(
        prepare_release_completion(ReleaseOutboxState { head: 1, tail: 257 }, &too_many).err(),
        Some(ReleaseOutboxError::InvalidCompletion)
    );
    assert_eq!(
        prepare_release_completion(state, &[1, 2])?
            .sequences()
            .count(),
        0
    );
    Ok(())
}
#[test]
fn release_intent_fields_preserve_nil_empty_partition_and_binary_ids() -> TestResult {
    let flow = vec![b'f'; 4096];
    let intent = ReleaseIntentRef {
        flow_id: &flow,
        partition: None,
        scope: b"scope",
        shard_id: 0,
        reservation_id: b"\0\xff",
        enforcement: LimitEnforcement::StrictGlobal,
        created_at: TimestampMillis::new(0),
    };
    validate_release_intent(intent)?;
    validate_release_intent(ReleaseIntentRef {
        partition: Some(b""),
        ..intent
    })?;
    for invalid in [
        ReleaseIntentRef {
            flow_id: b"",
            ..intent
        },
        ReleaseIntentRef {
            scope: b"",
            ..intent
        },
        ReleaseIntentRef {
            reservation_id: b"",
            ..intent
        },
        ReleaseIntentRef {
            shard_id: MAX + 1,
            ..intent
        },
        ReleaseIntentRef {
            created_at: TimestampMillis::new(MAX + 1),
            ..intent
        },
    ] {
        assert_eq!(
            validate_release_intent(invalid),
            Err(ReleaseOutboxError::InvalidIntent)
        );
    }
    let large = vec![0; 65_536];
    assert!(
        validate_release_intent(ReleaseIntentRef {
            partition: Some(&large),
            ..intent
        })
        .is_err()
    );
    assert!(
        validate_release_intent(ReleaseIntentRef {
            scope: &large,
            ..intent
        })
        .is_err()
    );
    assert!(
        validate_release_intent(ReleaseIntentRef {
            reservation_id: &[0; 257],
            ..intent
        })
        .is_err()
    );
    Ok(())
}
#[test]
fn release_classification_distinguishes_missing_running_sibling_and_malformed_status() {
    use ReleaseDisposition as D;
    use ReleaseRecordObservation as O;
    assert_eq!(classify_release(b"original", O::Missing), D::Release);
    assert_eq!(classify_release(b"original", O::Unavailable), D::ReadError);
    assert_eq!(classify_release(b"original", O::Corrupt), D::ReadError);
    assert_eq!(
        classify_release(
            b"original",
            O::Record {
                state: Some(b"running"),
                reservation_id: Some(b"original")
            }
        ),
        D::Retain
    );
    assert_eq!(
        classify_release(
            b"original",
            O::Record {
                state: Some(b"running"),
                reservation_id: Some(b"sibling")
            }
        ),
        D::Release
    );
    assert_eq!(
        classify_release(
            b"original",
            O::Record {
                state: Some(b"completed"),
                reservation_id: Some(b"original")
            }
        ),
        D::Release
    );
    assert_eq!(
        classify_release(
            b"original",
            O::Record {
                state: None,
                reservation_id: None
            }
        ),
        D::Release
    );
}
