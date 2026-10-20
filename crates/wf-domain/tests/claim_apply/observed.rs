//! Query-scoped observations retain the paid selected-Claim reduction.

use super::*;
use wf_domain::claim::apply::{
    ClaimObservedWindow, ClaimWindowEnd, decide_observed_claim, preflight_observed_claim_query,
};
use wf_domain::workflow::indexed::TouchedIndexedGroupView;

fn with_observed<'current, T>(
    full: ClaimApplyView<'current>,
    parallel: &[ClaimRecord<'current>],
    end: ClaimWindowEnd,
    heads_complete: bool,
    run: impl for<'query> FnOnce(ClaimObservedWindow<'query, 'current>) -> Result<T, TestError>,
) -> Result<T, TestError> {
    let current = full
        .group
        .records
        .iter()
        .map(|(id, row)| (*id, Some(*row)))
        .collect();
    let mut parallel_facts = BTreeMap::new();
    let mut heads = BTreeMap::new();
    let mut members = BTreeMap::new();
    for row in full.group.records.values() {
        if let Some(due) = DueOrder::for_record(row) {
            parallel_facts.insert(due, full.group.parallel.contains_key(&due));
        }
        if let Some(lane) = LaneKey::for_record(row) {
            heads.entry(lane).or_insert(None);
        }
    }
    let mut head_lanes = Vec::new();
    for (lane, rows) in full.group.fifo {
        let head = rows.first_key_value().map(|(order, row)| (*order, *row));
        heads.insert(*lane, head);
        head_lanes.push(*lane);
        if let Some((order, row)) = head {
            members.insert((*lane, order), Some(row));
        }
    }
    let policies = full
        .group
        .records
        .values()
        .map(|row| {
            let kind = row.record.workflow_type();
            (kind, full.group.policies.get(kind).copied())
        })
        .collect();
    let retries = full
        .group
        .records
        .values()
        .map(|row| (row.record.workflow_type(), None))
        .collect();
    run(ClaimObservedWindow {
        indexed: TouchedIndexedGroupView {
            group: full.group.batch.group,
            placements: full.group.batch.placements,
            current: &current,
            parallel: &parallel_facts,
            fifo_members: &members,
            fifo_heads: &heads,
            policies: &policies,
            retry_policies: &retries,
        },
        encodings: full.encodings,
        parallel,
        parallel_end: end,
        head_lanes: &head_lanes,
        heads_complete,
    })
}

fn selected_ids(decision: &ClaimDecision<'_>) -> Result<Vec<Vec<u8>>, TestError> {
    let ClaimDecision::Accepted(plan) = decision else {
        return Err("expected accepted observed decision".into());
    };
    Ok(plan.batch().map_or_else(Vec::new, |batch| {
        batch
            .batch()
            .items()
            .iter()
            .filter_map(|item| {
                item.plan()
                    .write()
                    .map(|write| write.record.id().as_bytes().to_vec())
            })
            .collect()
    }))
}

#[test]
fn observed_query_scope_matches_complete_reference_due_only() -> TestResult {
    let rows = [
        test_record(b"future-high", b"job", b"queued", None, 0, 100)?,
        test_record(b"due-low", b"job", b"queued", None, 0, 10)?,
        test_record(b"foreign", b"other", b"queued", None, 0, 0)?,
    ];
    let entries = rows
        .iter()
        .enumerate()
        .map(|(index, record)| ClaimRecord {
            record,
            partition_key: b"p",
            priority: if index == 1 { 0 } else { 2 },
            state_enter_seq: index as u128,
        })
        .collect::<Vec<_>>();
    let scope = test_scope()?;
    let request = RequestId::new(b"observed-scoped".to_vec())?;
    let partitions = [PartitionId::new(b"p".to_vec())?];
    let outcomes = BTreeMap::new();
    let mut envelope = test_envelope(test_header(&scope, &request, 1, 25)?, test_query(b"job", 1));
    envelope.scan_budget = 1;
    // A separate complete cohort for the defined, due query universe. Every
    // canonical and index map contains exactly this one relevant record.
    let reference_ids = with_claim_view(
        &entries[1..2],
        &partitions,
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::new(),
        1,
        |reference| {
            let old = decide_claim_request(
                reference,
                &envelope,
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            let DispatchDecision::Fresh(decision) = old.decision() else {
                return Err("complete reference was not Fresh".into());
            };
            selected_ids(decision)
        },
    )?;
    with_claim_view(
        &entries,
        &partitions,
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::new(),
        1,
        |full| {
            let (_, first_work) = preflight_observed_claim_query(
                &envelope,
                TimestampMillis::new(25),
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?
            .map_err(|reason| reason.to_string())?;
            let parallel = [entries[1]];
            with_observed(full, &parallel, ClaimWindowEnd::RangeEnd, true, |window| {
                let observed = decide_observed_claim(
                    window,
                    &envelope,
                    TimestampMillis::new(25),
                    AdmissionLimits::default(),
                    ClaimWorkLimits::default(),
                    first_work,
                )?;
                assert_eq!(selected_ids(&observed)?, [b"due-low".to_vec()]);
                assert_eq!(selected_ids(&observed)?, reference_ids);
                let ClaimDecision::Accepted(plan) = observed else {
                    return Err("no plan".into());
                };
                assert_eq!(
                    (plan.scanned(), plan.head_reads(), plan.stop()),
                    (1, 0, SelectionStop::Complete)
                );
                assert_eq!(plan.batch().ok_or("no batch")?.consumed_ordinals(), 0);
                Ok(())
            })
        },
    )
}

#[test]
fn observed_heads_block_and_touched_selected_guards() -> TestResult {
    let head = test_record(b"z-head", b"job", b"queued", None, 0, 100)?;
    let follower = test_record(b"a-follower", b"job", b"queued", None, 0, 1)?;
    let entries = [&head, &follower].map(|record| ClaimRecord {
        record,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: u128::from(record.id().as_bytes() == b"a-follower") + 1,
    });
    let policy = test_fifo_policy(&[b"queued"])?;
    let policies = BTreeMap::from([(b"job".as_slice(), policy.policy())]);
    let scope = test_scope()?;
    let request = RequestId::new(b"observed-head".to_vec())?;
    with_claim_view(
        &entries,
        &[PartitionId::new(b"p".to_vec())?],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &BTreeMap::new(),
        },
        &policies,
        1,
        |full| {
            let envelope =
                test_envelope(test_header(&scope, &request, 1, 25)?, test_query(b"job", 2));
            with_observed(full, &[], ClaimWindowEnd::RangeEnd, true, |window| {
                assert_eq!(
                    authority_controls(window, &envelope, entries[1])?,
                    [true; 3]
                );
                let decision = decide_observed_claim(
                    window,
                    &envelope,
                    TimestampMillis::new(25),
                    AdmissionLimits::default(),
                    ClaimWorkLimits::default(),
                    0,
                )?;
                assert!(
                    selected_ids(&decision)?.is_empty(),
                    "future true head blocks due follower"
                );
                let ClaimDecision::Accepted(plan) = decision else {
                    return Err("no empty plan".into());
                };
                assert_eq!(
                    (plan.head_reads(), plan.scanned(), plan.stop()),
                    (1, 0, SelectionStop::Complete)
                );
                let mut due = envelope;
                due.header.stamp = test_stamp(100, 100)?;
                let accepted = decide_observed_claim(
                    window,
                    &due,
                    TimestampMillis::new(100),
                    AdmissionLimits::default(),
                    ClaimWorkLimits::default(),
                    0,
                )?;
                assert_eq!(selected_ids(&accepted)?, [b"z-head".to_vec()]);
                let missing = BTreeMap::new();
                assert!(matches!(
                    decide_observed_claim(
                        ClaimObservedWindow {
                            indexed: TouchedIndexedGroupView {
                                fifo_members: &missing,
                                ..window.indexed
                            },
                            ..window
                        },
                        &due,
                        TimestampMillis::new(100),
                        AdmissionLimits::default(),
                        ClaimWorkLimits::default(),
                        0
                    ),
                    Err(ApplyError::InvalidAuthority)
                ));
                let no_current = BTreeMap::new();
                assert!(matches!(
                    decide_observed_claim(
                        ClaimObservedWindow {
                            indexed: TouchedIndexedGroupView {
                                current: &no_current,
                                ..window.indexed
                            },
                            ..window
                        },
                        &due,
                        TimestampMillis::new(100),
                        AdmissionLimits::default(),
                        ClaimWorkLimits::default(),
                        0
                    ),
                    Err(ApplyError::InvalidAuthority)
                ));
                Ok(())
            })
        },
    )
}

fn authority_controls(
    window: ClaimObservedWindow<'_, '_>,
    envelope: &ClaimEnvelope<'_>,
    wrong_member: ClaimRecord<'_>,
) -> Result<[bool; 3], TestError> {
    let wrong_members = window
        .indexed
        .fifo_members
        .keys()
        .map(|key| (*key, Some(wrong_member)))
        .collect();
    let wrong = ClaimObservedWindow {
        indexed: TouchedIndexedGroupView {
            fifo_members: &wrong_members,
            ..window.indexed
        },
        ..window
    };
    let member_refused = authority_refused(wrong, envelope);
    let heads = window.head_lanes.iter().map(|lane| (*lane, None)).collect();
    let empty_current = BTreeMap::new();
    let empty_members = BTreeMap::new();
    let empty_encodings = BTreeMap::new();
    let empty = ClaimObservedWindow {
        indexed: TouchedIndexedGroupView {
            current: &empty_current,
            fifo_members: &empty_members,
            fifo_heads: &heads,
            ..window.indexed
        },
        encodings: &empty_encodings,
        ..window
    };
    let positive = decide_observed_claim(
        empty,
        envelope,
        TimestampMillis::new(25),
        AdmissionLimits::default(),
        ClaimWorkLimits::default(),
        0,
    )?;
    assert!(
        selected_ids(&positive)?.is_empty(),
        "healthy explicit empty lane"
    );
    let missing = BTreeMap::new();
    let missing_refused = authority_refused(
        ClaimObservedWindow {
            indexed: TouchedIndexedGroupView {
                placements: &missing,
                ..empty.indexed
            },
            ..empty
        },
        envelope,
    );
    let foreign = window
        .indexed
        .placements
        .keys()
        .map(|partition| {
            Ok((
                partition.clone(),
                wf_types::identity::GroupId::new(b"foreign".to_vec())?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, TestError>>()?;
    let foreign_refused = authority_refused(
        ClaimObservedWindow {
            indexed: TouchedIndexedGroupView {
                placements: &foreign,
                ..empty.indexed
            },
            ..empty
        },
        envelope,
    );
    Ok([member_refused, missing_refused, foreign_refused])
}

fn authority_refused(window: ClaimObservedWindow<'_, '_>, envelope: &ClaimEnvelope<'_>) -> bool {
    matches!(
        decide_observed_claim(
            window,
            envelope,
            TimestampMillis::new(25),
            AdmissionLimits::default(),
            ClaimWorkLimits::default(),
            0
        ),
        Err(ApplyError::InvalidAuthority)
    )
}

#[test]
fn observed_window_frontier_and_work_refuse_truthfully() -> TestResult {
    let row = test_record(b"id", b"job", b"queued", None, 0, 0)?;
    let second = test_record(b"zz", b"job", b"queued", None, 0, 0)?;
    let entries = [&row, &second].map(|record| ClaimRecord {
        record,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: 11,
    });
    let scope = test_scope()?;
    let request = RequestId::new(b"work".to_vec())?;
    with_claim_view(
        &entries[..1],
        &[PartitionId::new(b"p".to_vec())?],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &BTreeMap::new(),
        },
        &BTreeMap::new(),
        1,
        |full| {
            let mut envelope =
                test_envelope(test_header(&scope, &request, 1, 25)?, test_query(b"job", 1));
            envelope.scan_budget = 1;
            let (q, first_work) = preflight_observed_claim_query(
                &envelope,
                TimestampMillis::new(25),
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?
            .map_err(|e| e.to_string())?;
            assert_eq!((q, first_work), (11, 2827));
            with_observed(
                full,
                &entries[..1],
                ClaimWindowEnd::RangeEnd,
                true,
                |window| assert_exact_work(window, &envelope, first_work),
            )
        },
    )?;
    with_claim_view(
        &entries,
        &[PartitionId::new(b"p".to_vec())?],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &BTreeMap::new(),
        },
        &BTreeMap::new(),
        1,
        |full| {
            let mut envelope =
                test_envelope(test_header(&scope, &request, 1, 25)?, test_query(b"job", 1));
            envelope.scan_budget = 1;
            let (_, first_work) = preflight_observed_claim_query(
                &envelope,
                TimestampMillis::new(25),
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?
            .map_err(|e| e.to_string())?;
            envelope.query.limit = 2;
            with_observed(full, &entries, ClaimWindowEnd::PrefixEnd, true, |window| {
                let decision = decide_observed_claim(
                    window,
                    &envelope,
                    TimestampMillis::new(25),
                    AdmissionLimits::default(),
                    ClaimWorkLimits::default(),
                    first_work,
                )?;
                let ClaimDecision::Accepted(plan) = decision else {
                    return Err("no partial plan".into());
                };
                assert_eq!(
                    (plan.scanned(), plan.stop()),
                    (1, SelectionStop::ScanBudget)
                );
                assert_eq!(
                    plan.batch()
                        .ok_or("no partial batch")?
                        .batch()
                        .items()
                        .len(),
                    1
                );
                Ok(())
            })
        },
    )
}

fn assert_exact_work(
    window: ClaimObservedWindow<'_, '_>,
    envelope: &ClaimEnvelope<'_>,
    first_work: u64,
) -> TestResult {
    // Independent fixed-fixture tariff: g1+g2, due4096, scan1024,
    // consumed4674, selected66 and plan1272. No oracle calls new code.
    let exact = 2 * 2827 + 4096 + 1024 + 4674 + 66 + 1272;
    let decision = decide_observed_claim(
        window,
        envelope,
        TimestampMillis::new(25),
        AdmissionLimits::default(),
        ClaimWorkLimits::new(exact)?,
        first_work,
    )?;
    assert_eq!(selected_ids(&decision)?, [b"id".to_vec()]);
    assert!(
        matches!(decide_observed_claim(window, envelope, TimestampMillis::new(25),
        AdmissionLimits::default(), ClaimWorkLimits::new(exact - 1)?, first_work)?,
        ClaimDecision::Rejected(ClaimRejection::WorkBytes { bytes, maximum })
            if bytes == exact && maximum == exact - 1)
    );
    assert!(matches!(
        decide_observed_claim(
            ClaimObservedWindow {
                heads_complete: false,
                ..window
            },
            envelope,
            TimestampMillis::new(25),
            AdmissionLimits::default(),
            ClaimWorkLimits::default(),
            first_work
        )?,
        ClaimDecision::Rejected(ClaimRejection::HeadBudget)
    ));
    Ok(())
}
