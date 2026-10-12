//! Restored unscheduled FIFO member and coherent claim dispatch.
use super::super::*;
use wf_domain::claim::apply::{
    ClaimApplyView, ClaimEncoding, ClaimEnvelope, ClaimQuery, ClaimStates, ClaimWorkLimits,
};
use wf_domain::claim::{PartitionFilter, SelectionStop};
use wf_domain::workflow::control::{
    ControlState, EntryIdentity, EntryStamp, PreparedDigest, plan_control,
};
use wf_domain::workflow::request::gate::{CommittedHeader, GateView};
use wf_domain::workflow::request::{
    ClientIntent, DispatchDecision, RequestScope, decide_claim_request,
};
use wf_types::identity::{RecoveryId, RequestId, TenantId};
use wf_types::version::{CommittedTime, HlcTimestamp};

mod indexed;

#[test]
fn unscheduled_fifo_parent_blocks_followers_and_injected_head_refetches() -> TestResult {
    let captured = fixtures::lifecycle(MaxActiveOverride::Infinite, 100, 0, 10)?;
    let records = [
        restore_active(
            fixture(
                b"head",
                b"p",
                b"waiting",
                TimestampMillis::new(0),
                None,
                captured,
            )?,
            AdmissionLimits::default(),
        )?,
        restore_active(
            fixture(
                b"follower",
                b"p",
                b"waiting",
                TimestampMillis::new(1),
                Some(TimestampMillis::new(1)),
                captured,
            )?,
            AdmissionLimits::default(),
        )?,
    ];
    let policy = restore_candidate(
        1,
        PolicySnapshotData {
            workflow_type: b"job".to_vec(),
            version: None,
            max_active_ms: None,
            retry: RetryOverride::default(),
            retention: RetentionOverride::default(),
            states: [(
                b"waiting".to_vec(),
                StatePolicy {
                    mode: Some(StateMode::Fifo),
                    retry: RetryOverride::default(),
                    retention: RetentionOverride::default(),
                },
            )]
            .into(),
        },
        PolicyLimits::default(),
    )?;
    let policies = [(b"job".to_vec(), policy.policy().clone())].into();
    with_view(
        &records,
        &policies,
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let rows = active_rows(&records[0], StateMode::Fifo)?;
            assert!(rows.fifo.is_some_and(|(_, _, due)| due.is_none()));
            assert!(rows.parallel.is_none());
            assert_blocked_claim(view)?;
            let ActiveTimeoutDecision::Accepted(expiry) = expire_at(
                view,
                &[ScopedRecordId {
                    partition_key: b"p",
                    id: b"head",
                }],
                TimestampMillis::new(5),
                EntryOrder {
                    identity: LogIdentity::new(1, 7, AppliedPosition::new(2)),
                    first_ordinal: 0,
                },
                (AdmissionLimits::default(), TimeoutLimits::default()),
            )?
            else {
                return Err("infinite head rejected".into());
            };
            assert!(expiry.writes().is_empty());
            assert_eq!(
                expiry
                    .record(ScopedRecordId {
                        partition_key: b"p",
                        id: b"head"
                    })
                    .ok_or("head")?
                    .core()
                    .next_run_at(),
                None
            );
            Ok(())
        },
    )?;
    assert_timeout_head_refetch(&policies)?;
    indexed::assert_generic_unscheduled_fifo_deltas(&policies)?;
    indexed::assert_generic_scheduled_fifo_entry(&policies)?;
    indexed::assert_generic_unscheduled_fifo_authority(&policies)?;
    indexed::assert_generic_unscheduled_fifo_noops(&policies)?;
    indexed::assert_generic_unscheduled_fifo_budget(&policies)?;
    assert_running_overlay_head()
}

fn assert_timeout_head_refetch(policies: &BTreeMap<Vec<u8>, PolicySnapshot>) -> TestResult {
    let finite = fixtures::lifecycle(MaxActiveOverride::Millis(1), 100, 0, 10)?;
    let infinite = fixtures::lifecycle(MaxActiveOverride::Infinite, 100, 0, 10)?;
    let zero = TimestampMillis::new(0);
    let one = TimestampMillis::new(1);
    let records = [
        restore_active(
            fixture(b"head", b"p", b"waiting", zero, None, finite)?,
            AdmissionLimits::default(),
        )?,
        restore_active(
            fixture(b"follower", b"p", b"waiting", one, Some(one), infinite)?,
            AdmissionLimits::default(),
        )?,
    ];
    let originals = records.clone();
    let order = EntryOrder {
        identity: LogIdentity::new(1, 7, AppliedPosition::new(2)),
        first_ordinal: 0,
    };
    let profiles = (AdmissionLimits::default(), TimeoutLimits::default());
    with_view(
        &records,
        policies,
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let root = ScopedRecordId {
                partition_key: b"p",
                id: b"head",
            };
            let follower = ScopedRecordId {
                partition_key: b"p",
                id: b"follower",
            };
            let ActiveTimeoutDecision::Accepted(plan) =
                expire_at(view, &[root], one, order, profiles)?
            else {
                return Err("finite unscheduled head rejected".into());
            };
            assert_eq!((plan.stats(), plan.writes().len()), (1, 1));
            let heads: Vec<_> = plan.affected_fifo_heads().collect();
            assert_eq!(heads.len(), 1);
            let head = heads[0].1.ok_or("follower head")?;
            assert_eq!(head, view.indexed.records[&follower]);
            assert!(std::ptr::eq(head.record, records[1].core()));
            assert!(std::ptr::eq(
                plan.record(follower).ok_or("follower refetch")?,
                view.records[&follower]
            ));
            let deltas: Vec<_> = plan.deltas().collect::<Result<_, _>>()?;
            assert_eq!(deltas.len(), 1);
            assert!(deltas[0].before.as_ref().is_some_and(|r| r.fifo.is_some()));
            assert!(deltas[0].after.as_ref().is_some_and(|r| r.fifo.is_none()));
            let mut lanes = view.indexed.fifo.clone();
            let entry = view.indexed.records[&follower];
            let lane = LaneKey::for_record(&entry).ok_or("lane")?;
            lanes
                .get_mut(&lane)
                .ok_or("members")?
                .get_mut(&LaneOrder::for_record(&entry))
                .ok_or("member")?
                .priority = 1;
            let corrupt = ActiveTimeoutView {
                indexed: IndexedGroupView {
                    fifo: &lanes,
                    ..view.indexed
                },
                ..view
            };
            assert_no_plan(
                expire_at(corrupt, &[root], one, order, profiles),
                Err(ApplyError::InvalidAuthority),
            )
        },
    )?;
    assert!(records == originals);
    Ok(())
}

fn assert_running_overlay_head() -> TestResult {
    let finite = fixtures::lifecycle(MaxActiveOverride::Millis(1), 100, 0, 10)?;
    let infinite = fixtures::lifecycle(MaxActiveOverride::Infinite, 100, 0, 10)?;
    let zero = TimestampMillis::new(0);
    let mut data = fixtures::family_data(finite, infinite, b"running", (b"p", b"p"))?;
    let group = data[0]
        .child_groups
        .get_mut(b"g".as_slice())
        .ok_or("group")?;
    group.on_child_failed = ChildFailure::Ignore;
    group.on_parent_closed = ParentClosed::Abandon;
    let sequence = u128::MAX - 1;
    data[0].ordering.state_enter_seq = sequence;
    data[2].core = fixture(b"sibling", b"p", b"queued", zero, Some(zero), infinite)?.core;
    data[2].captured = infinite;
    let records = data
        .into_iter()
        .map(|data| Ok(restore_active(data, AdmissionLimits::default())?))
        .collect::<TestResult<Vec<_>>>()?;
    let originals = records.clone();
    let policy = restore_candidate(
        1,
        PolicySnapshotData {
            workflow_type: b"job".to_vec(),
            version: None,
            max_active_ms: None,
            retry: RetryOverride::default(),
            retention: RetentionOverride::default(),
            states: [(
                b"queued".to_vec(),
                StatePolicy {
                    mode: Some(StateMode::Fifo),
                    ..StatePolicy::default()
                },
            )]
            .into(),
        },
        PolicyLimits::default(),
    )?;
    let policies = [(b"job".to_vec(), policy.policy().clone())].into();
    with_view(
        &records,
        &policies,
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let parent_id = ScopedRecordId {
                partition_key: b"p",
                id: b"parent",
            };
            let ActiveTimeoutDecision::Accepted(plan) = expire_at(
                view,
                &[ScopedRecordId {
                    partition_key: b"p",
                    id: b"child",
                }],
                TimestampMillis::new(1),
                EntryOrder {
                    identity: LogIdentity::new(1, 7, AppliedPosition::new(2)),
                    first_ordinal: 0,
                },
                (AdmissionLimits::default(), TimeoutLimits::default()),
            )?
            else {
                return Err("running head rejected".into());
            };
            assert_eq!(
                (
                    plan.writes().len(),
                    plan.history().len(),
                    plan.consumed_ordinals()
                ),
                (2, 2, 0)
            );
            let parent = plan.record(parent_id).ok_or("parent")?;
            assert_eq!(parent.core().lease(), records[0].core().lease());
            assert_eq!(parent.core().next_run_at(), Some(TimestampMillis::new(50)));
            let heads: Vec<_> = plan.affected_fifo_heads().collect();
            assert_eq!(heads.len(), 1);
            let head = heads[0].1.ok_or("running head")?;
            assert!(std::ptr::eq(head.record, parent.core()));
            assert_eq!(head.logical_state(), b"queued");
            assert_eq!(head.state_enter_seq, sequence);
            assert!(plan.deltas().all(|d| d.is_ok()));
            Ok(())
        },
    )?;
    assert!(records == originals);
    Ok(())
}

fn blocked_claim_header<'a>(
    scope: &'a RequestScope,
    request: &'a RequestId,
) -> TestResult<CommittedHeader<'a>> {
    Ok(CommittedHeader {
        format_version: 1,
        scope,
        request,
        intent: ClientIntent::new(1, [1; 32])?,
        identity: EntryIdentity::new(
            LogIdentity::new(1, 7, AppliedPosition::new(1)),
            PreparedDigest::new([2; 32]),
        ),
        stamp: CommittedTime::new(HlcTimestamp::new(5, 0)?, 5)?,
        encoded_entry: b"claim",
        encoded_isolation_scope: b"scope",
    })
}

fn assert_blocked_claim(view: ActiveTimeoutView<'_>) -> TestResult {
    let scope = RequestScope::new(
        TenantId::new(b"tenant".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"epoch".to_vec())?,
    );
    let request = RequestId::new(b"claim".to_vec())?;
    let genesis = EntryIdentity::new(
        LogIdentity::new(1, 7, AppliedPosition::new(0)),
        PreparedDigest::new([1; 32]),
    );
    let control = plan_control(
        ControlState::initial(CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?),
        genesis,
        EntryStamp::Unstamped,
    )?
    .next();
    let outcomes = BTreeMap::new();
    let encodings: BTreeMap<_, _> = view
        .records
        .iter()
        .map(|(id, record)| {
            (
                *id,
                ClaimEncoding {
                    partition: record.partition(),
                    state_key: id.id,
                },
            )
        })
        .collect();
    let header = blocked_claim_header(&scope, &request)?;
    let envelope = ClaimEnvelope {
        header,
        query: ClaimQuery {
            workflow_type: b"job",
            partitions: PartitionFilter::Any,
            states: ClaimStates::Only(&[b"waiting"]),
            exclude_states: &[],
            priority: None,
            worker: b"worker",
            lease_ms: 10,
            limit: 1,
        },
        token_version: 1,
        first_ordinal: 0,
        scan_budget: 10,
        head_budget: 10,
        resume: None,
    };
    let result = decide_claim_request(
        ClaimApplyView {
            gate: GateView {
                scope: &scope,
                control,
                outcomes: &outcomes,
            },
            group: view.indexed,
            revision: 1,
            encodings: &encodings,
        },
        &envelope,
        AdmissionLimits::default(),
        ClaimWorkLimits::default(),
    )?;
    let DispatchDecision::Fresh(wf_domain::claim::apply::ClaimDecision::Accepted(plan)) =
        result.decision()
    else {
        return Err("blocked claim rejected".into());
    };
    assert!(plan.batch().is_none());
    assert_eq!(plan.stop(), SelectionStop::Complete);
    assert_eq!(plan.head_reads(), 1);
    Ok(())
}
