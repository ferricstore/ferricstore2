//! Existing priority survives policy activation; a lane has one global head.

use super::*;
use wf_domain::claim::apply::{
    ClaimObservedWindow, ClaimPlan, ClaimWindowEnd, decide_observed_claim,
};
use wf_domain::lease::ExtendLease;
use wf_domain::workflow::batch::BatchItem;
use wf_domain::workflow::indexed::{
    EntryOrder, IndexedBatchItem, IndexedDecision, TouchedIndexedGroupView, decide_indexed_batch,
};

#[test]
fn activated_nonzero_fifo_priority_preserves_claim_and_lease_mutations() -> TestResult {
    for priority in [1, 2] {
        let initial = test_record(b"head", b"job", b"queued", Some(vec![0, 255]), 0, 0)?;
        let policy = test_fifo_policy(&[b"queued"])?;
        let running = claim_activated_head(&initial, priority, policy.policy())?;
        assert_eq!(
            (running.version(), running.fence().get(), running.attempts()),
            (2, 1, 0)
        );
        assert_eq!(running.run_state(), Some(b"queued".as_slice()));
        assert_eq!(running.payload(), Some([0, 255].as_slice()));
        let extended = mutate_activated_head(
            &running,
            priority,
            policy.policy(),
            110,
            Operation::ExtendLease(ExtendLease {
                guard: activated_guard(&running)?,
                lease_ms: 100,
            }),
        )?;
        assert_eq!((extended.version(), extended.fence().get()), (3, 1));
        assert_eq!(extended.lease_deadline().get(), 210);
        let reclaimed = mutate_activated_head(
            &extended,
            priority,
            policy.policy(),
            211,
            Operation::Reclaim(Claim {
                expected_state: b"running".to_vec(),
                worker: b"next-worker".to_vec(),
                token: LeaseToken::new(vec![0, 255])?,
                lease_ms: 50,
            }),
        )?;
        assert_eq!(
            (
                reclaimed.version(),
                reclaimed.fence().get(),
                reclaimed.attempts()
            ),
            (4, 2, 0)
        );
        assert_eq!(reclaimed.lease_deadline().get(), 261);
        assert_eq!(
            reclaimed
                .lease()
                .ok_or("missing reclaimed lease")?
                .token
                .as_bytes(),
            [0, 255]
        );
        let completed = mutate_activated_head(
            &reclaimed,
            priority,
            policy.policy(),
            212,
            Operation::Complete(Terminal {
                guard: activated_guard(&reclaimed)?,
                payload: None,
                value: Some(b"done".to_vec()),
            }),
        )?;
        assert_eq!(
            (
                completed.state(),
                completed.version(),
                completed.fence().get()
            ),
            (b"completed".as_slice(), 5, 2)
        );
        assert_eq!(completed.result(), Some(b"done".as_slice()));
        assert_eq!(
            (initial.version(), initial.fence().get(), initial.state()),
            (1, 0, b"queued".as_slice())
        );
    }
    Ok(())
}

fn activated_guard(record: &WorkflowRecord) -> Result<LeaseGuard, TestError> {
    Ok(LeaseGuard {
        token: record
            .lease()
            .ok_or("missing activated lease")?
            .token
            .clone(),
        fence: record.fence(),
    })
}

fn claim_activated_head(
    record: &WorkflowRecord,
    priority: u8,
    policy: &PolicySnapshot,
) -> Result<WorkflowRecord, TestError> {
    let scope = test_scope()?;
    let request = RequestId::new(b"claim-activated".to_vec())?;
    let outcomes = BTreeMap::new();
    let entries = [ClaimRecord {
        record,
        partition_key: b"p",
        priority,
        state_enter_seq: 111,
    }];
    with_claim_view(
        &entries,
        &[PartitionId::new(b"p".to_vec())?],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::from([(b"job".as_slice(), policy)]),
        1,
        |view| {
            let query = ClaimQuery {
                priority: Some(priority),
                ..test_query(b"job", 1)
            };
            let plan = decide_claim_request(
                view,
                &test_envelope(test_header(&scope, &request, 1, 100)?, query),
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision() else {
                return Err("coherent activated head was not accepted".into());
            };
            assert_eq!((claim.scanned(), claim.head_reads()), (1, 1));
            let batch = claim.batch().ok_or("activated head not claimed")?;
            let metadata = batch.metadata().next().ok_or("missing claim ordering")?;
            assert_eq!(
                (metadata.priority, metadata.state_enter_seq),
                (priority, 111)
            );
            assert_eq!(batch.batch().items().len(), 1);
            let envelope = test_envelope(test_header(&scope, &request, 1, 100)?, query);
            let observed = observed_activated_claim(view, &envelope, entries[0], None, 100)?;
            let ClaimDecision::Accepted(observed) = observed else {
                return Err("observed activated head was not accepted".into());
            };
            let observed_batch = observed
                .batch()
                .ok_or("observed activated head not claimed")?;
            assert_eq!(observed_batch, batch);
            assert_eq!((observed.scanned(), observed.head_reads()), (1, 1));
            Ok(batch.batch().items()[0]
                .plan()
                .write()
                .ok_or("missing claim write")?
                .record
                .clone())
        },
    )
}

fn mutate_activated_head(
    record: &WorkflowRecord,
    priority: u8,
    policy: &PolicySnapshot,
    now: u64,
    operation: Operation,
) -> Result<WorkflowRecord, TestError> {
    let scope = test_scope()?;
    let outcomes = BTreeMap::new();
    let partition = PartitionId::new(b"p".to_vec())?;
    let command = CommittedCommand {
        id: record.id().clone(),
        now: TimestampMillis::new(now),
        operation,
    };
    let entries = [ClaimRecord {
        record,
        partition_key: b"p",
        priority,
        state_enter_seq: 111,
    }];
    with_claim_view(
        &entries,
        std::slice::from_ref(&partition),
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::from([(b"job".as_slice(), policy)]),
        1,
        |view| {
            let items = [IndexedBatchItem::new(BatchItem::new(
                &partition, &command, b"key",
            ))];
            let plan = decide_indexed_batch(
                view.group,
                &items,
                EntryOrder {
                    identity: test_identity(1).log(),
                    first_ordinal: 0,
                },
                b"entry",
                AdmissionLimits::default(),
            )?;
            let IndexedDecision::Accepted(batch) = plan else {
                return Err("activated lease/terminal mutation rejected".into());
            };
            let metadata = batch.metadata().next().ok_or("missing mutation ordering")?;
            assert_eq!(
                (metadata.priority, metadata.state_enter_seq),
                (priority, 111)
            );
            assert_eq!(batch.consumed_ordinals(), 0);
            assert_eq!(batch.batch().items()[0].plan().outcome(), Outcome::Changed);
            Ok(batch.batch().items()[0]
                .plan()
                .write()
                .ok_or("missing mutation write")?
                .record
                .clone())
        },
    )
}

#[test]
fn global_fifo_head_blocks_other_priority_without_spending_candidate_scan() -> TestResult {
    for head_priority in 0..=2 {
        for requested in 0..=2 {
            for scan_budget in [16_384, 1] {
                assert_global_head_priority(head_priority, requested, false, scan_budget)?;
                if requested != head_priority {
                    // B=1 must remain available to an eligible independent parallel row.
                    assert_global_head_priority(head_priority, requested, true, scan_budget)?;
                }
            }
        }
    }
    Ok(())
}

fn assert_global_head_priority(
    head_priority: u8,
    requested: u8,
    parallel: bool,
    scan_budget: usize,
) -> TestResult {
    let scope = test_scope()?;
    let outcomes = BTreeMap::new();
    let request = RequestId::new(b"priority-filter".to_vec())?;
    let policy = test_fifo_policy(&[b"queued"])?;
    let rows = [
        test_record(b"z-head", b"job", b"queued", None, 0, 0)?,
        test_record(b"a-follower", b"job", b"queued", None, 1, 0)?,
        test_record(b"parallel", b"job", b"ready", None, 2, 0)?,
    ];
    let entries = rows
        .iter()
        .enumerate()
        .take(if parallel { 3 } else { 2 })
        .map(|(index, record)| ClaimRecord {
            record,
            partition_key: b"p",
            priority: if index == 0 { head_priority } else { requested },
            state_enter_seq: index as u128 + 1,
        })
        .collect::<Vec<_>>();
    with_claim_view(
        &entries,
        &[PartitionId::new(b"p".to_vec())?],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::from([(b"job".as_slice(), policy.policy())]),
        1,
        |view| {
            let query = ClaimQuery {
                priority: Some(requested),
                ..test_query(b"job", 1)
            };
            let envelope = ClaimEnvelope {
                scan_budget,
                ..test_envelope(test_header(&scope, &request, 1, 10)?, query)
            };
            let plan = decide_claim_request(
                view,
                &envelope,
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision() else {
                return Err("global FIFO head/filter query rejected".into());
            };
            assert_priority_selection(claim, requested == head_priority, parallel)?;
            let parallel_row = if parallel { Some(entries[2]) } else { None };
            let observed = observed_activated_claim(view, &envelope, entries[0], parallel_row, 10)?;
            let ClaimDecision::Accepted(observed) = observed else {
                return Err("observed global-head/filter query rejected".into());
            };
            assert_priority_selection(&observed, requested == head_priority, parallel)?;
            assert!(
                rows.iter()
                    .all(|record| record.version() == 1 && record.lease().is_none())
            );
            Ok(())
        },
    )
}

fn assert_priority_selection(claim: &ClaimPlan<'_>, matches: bool, parallel: bool) -> TestResult {
    assert_eq!(claim.head_reads(), 1);
    if matches || parallel {
        let batch = claim
            .batch()
            .ok_or("eligible head/parallel row was not claimed")?;
        assert_eq!(batch.batch().items().len(), 1);
        let actual = batch.batch().items()[0]
            .plan()
            .write()
            .ok_or("missing selected write")?;
        let expected = if parallel {
            b"parallel".as_slice()
        } else {
            b"z-head".as_slice()
        };
        assert_eq!(actual.record.id().as_bytes(), expected);
        assert_eq!(claim.scanned(), 1);
    } else {
        assert!(claim.batch().is_none());
        assert_eq!(
            (claim.scanned(), claim.stop()),
            (0, SelectionStop::Complete)
        );
    }
    Ok(())
}

fn observed_activated_claim<'current>(
    full: ClaimApplyView<'current>,
    envelope: &ClaimEnvelope<'_>,
    head: ClaimRecord<'current>,
    parallel: Option<ClaimRecord<'current>>,
    now: u64,
) -> Result<ClaimDecision<'current>, TestError> {
    // These explicit reached facts are separate from the Full reference indexes.
    // The queued lane is FIFO; ready is Parallel. A real global head is supplied
    // even when its priority does not match. RangeEnd is the entire relevant
    // parallel stream of these finite fixtures, never a budget-exhausted prefix.
    let current = full
        .group
        .records
        .iter()
        .map(|(id, row)| (*id, Some(*row)))
        .collect();
    let mut parallel_facts = BTreeMap::new();
    let mut members = BTreeMap::new();
    let mut heads = BTreeMap::new();
    for row in full.group.records.values() {
        if let Some(due) = DueOrder::for_record(row) {
            parallel_facts.insert(due, row.logical_state() == b"ready");
        }
        if let Some(lane) = LaneKey::for_record(row) {
            members.insert(
                (lane, LaneOrder::for_record(row)),
                (row.logical_state() == b"queued").then_some(*row),
            );
            heads.entry(lane).or_insert(None);
        }
    }
    let lane = LaneKey::for_record(&head).ok_or("missing explicit head lane")?;
    heads.insert(lane, Some((LaneOrder::for_record(&head), head)));
    let policies = BTreeMap::from([(
        b"job".as_slice(),
        full.group.policies.get(b"job".as_slice()).copied(),
    )]);
    let retries = BTreeMap::from([(b"job".as_slice(), None)]);
    let parallel = parallel.into_iter().collect::<Vec<_>>();
    Ok(decide_observed_claim(
        ClaimObservedWindow {
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
            parallel: &parallel,
            parallel_end: ClaimWindowEnd::RangeEnd,
            head_lanes: &[lane],
            heads_complete: true,
        },
        envelope,
        TimestampMillis::new(now),
        AdmissionLimits::default(),
        ClaimWorkLimits::default(),
        0,
    )?)
}
