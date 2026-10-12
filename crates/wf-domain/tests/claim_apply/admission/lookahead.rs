//! Supplemental occurrence boundaries derived independently from real fixtures.

use super::*;

pub(super) fn check_repeated_lookahead_charge(
    view: ClaimApplyView<'_>,
    scope: &RequestScope,
    request: &RequestId,
) -> TestResult {
    let rows = [
        test_record(b"a", b"job", b"queued", None, 0, 0)?,
        test_record(b"b", b"job", b"queued", None, 0, 0)?,
        test_record(b"c", b"other", b"queued", None, 0, 1)?,
    ];
    let entries: Vec<_> = rows
        .iter()
        .zip([b"p".as_slice(), b"q", b"p"])
        .map(|(record, partition_key)| ClaimRecord {
            record,
            partition_key,
            priority: 0,
            state_enter_seq: 1,
        })
        .collect();
    let policy = test_fifo_policy(&[b"queued"])?;
    let policies = BTreeMap::from([(b"job".as_slice(), policy.policy())]);
    with_claim_view(
        &entries,
        &[
            PartitionId::new(b"p".to_vec())?,
            PartitionId::new(b"q".to_vec())?,
        ],
        view.gate,
        &policies,
        view.revision,
        |coherent| {
            let envelope =
                test_envelope(test_header(scope, request, 1, 25)?, test_query(b"job", 2));
            // Q11, R10/B48/F20/D60/T100 per head; N91, G29771, P2636.
            // Both FIFO heads win; c is peeked twice and never popped.
            for (maximum, expected_bytes) in [
                (32_407, None),
                (32_406, Some(32_407)),
                (28_055, Some(29_571)),
            ] {
                let plan = decide_claim_request(
                    coherent,
                    &envelope,
                    AdmissionLimits::default(),
                    ClaimWorkLimits::new(maximum)?,
                )?;
                match (plan.decision(), expected_bytes) {
                    (DispatchDecision::Fresh(ClaimDecision::Accepted(claim)), None) => {
                        let batch = claim.batch().ok_or("missing two FIFO claims")?;
                        assert_eq!(
                            (
                                claim.head_reads(),
                                claim.scanned(),
                                batch.batch().items().len()
                            ),
                            (2, 2, 2)
                        );
                        assert_eq!(claim.stop(), SelectionStop::ClaimLimit);
                        assert_eq!(
                            batch
                                .batch()
                                .items()
                                .iter()
                                .map(|item| {
                                    item.plan()
                                        .write()
                                        .map(|write| write.record.id().as_bytes())
                                })
                                .collect::<Vec<_>>(),
                            [Some(b"a".as_slice()), Some(b"b".as_slice())]
                        );
                    }
                    (
                        DispatchDecision::Fresh(ClaimDecision::Rejected(
                            ClaimRejection::WorkBytes {
                                bytes,
                                maximum: actual_maximum,
                            },
                        )),
                        Some(expected),
                    ) => assert_eq!((*bytes, *actual_maximum), (expected, maximum)),
                    _ => return Err("repeated lookahead boundary failed".into()),
                }
            }
            assert!(rows.iter().all(|record| record.version() == 1));
            Ok(())
        },
    )
}

pub(super) fn check_blocked_head_admission(
    view: ClaimApplyView<'_>,
    scope: &RequestScope,
    request: &RequestId,
) -> TestResult {
    let huge = vec![b'x'; 65_536];
    let physical = [b"waiting".as_slice()];
    let query = ClaimQuery {
        states: ClaimStates::Only(&physical),
        ..test_query(b"job", 1)
    };
    // R33/Q18/N46/D198 give G14033/P72; the first scan needs14033.
    let boundaries = [
        (14_105, None),
        (14_104, Some(14_105)),
        (14_032, Some(14_033)),
    ];
    let unrestricted = [(u64::MAX, None)];
    for (state, payload, inline) in [
        (b"queued".as_slice(), None, 1_048_576),
        (huge.as_slice(), None, 100_000),
        (b"queued".as_slice(), Some(vec![0; 100]), 99),
    ] {
        let invalid_value = payload.is_some();
        let queued = test_record(b"a", b"job", state, payload, 0, 0)?;
        let record = test_claimed(&queued, b"old", 0, 100)?;
        let before = record.clone();
        let entries = [ClaimRecord {
            record: &record,
            partition_key: b"p",
            priority: 0,
            state_enter_seq: 1,
        }];
        let policy = test_fifo_policy(&[state])?;
        let policies = BTreeMap::from([(b"job".as_slice(), policy.policy())]);
        with_claim_view(
            &entries,
            &[PartitionId::new(b"p".to_vec())?],
            view.gate,
            &policies,
            view.revision,
            |coherent| {
                let cases = if state.len() <= 65_535 && !invalid_value {
                    &boundaries[..]
                } else {
                    &unrestricted[..]
                };
                for &(maximum, expected_bytes) in cases {
                    let result = decide_claim_request(
                        coherent,
                        &test_envelope(test_header(scope, request, 1, 25)?, query),
                        AdmissionLimits::new(inline, 64 * 1_048_576, 1)?,
                        ClaimWorkLimits::new(maximum)?,
                    );
                    if invalid_value {
                        assert!(matches!(result, Err(ApplyError::InvalidAuthority)));
                    } else {
                        let plan = result?;
                        let expected = if state.len() > 65_535 {
                            Some(ClaimRejection::ValueTooLarge {
                                bytes: 65_536,
                                maximum: 65_535,
                            })
                        } else {
                            expected_bytes.map(|bytes| ClaimRejection::WorkBytes { bytes, maximum })
                        };
                        match (plan.decision(), expected) {
                            (DispatchDecision::Fresh(ClaimDecision::Accepted(claim)), None) => {
                                assert!(claim.batch().is_none());
                                assert_eq!(
                                    (claim.head_reads(), claim.scanned(), claim.stop()),
                                    (1, 1, SelectionStop::Complete)
                                );
                                assert!(plan.requires_outcome_write());
                            }
                            (
                                DispatchDecision::Fresh(ClaimDecision::Rejected(actual)),
                                Some(expected),
                            ) => assert_eq!(*actual, expected),
                            _ => return Err("blocked FIFO head boundary failed".into()),
                        }
                    }
                }
                Ok(())
            },
        )?;
        assert_eq!(record, before);
        assert_eq!(record.lease_deadline(), TimestampMillis::new(100));
    }
    Ok(())
}
