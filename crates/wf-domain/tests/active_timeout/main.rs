//! Complete pure timeout closures; the model helper performs no persistence.
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use wf_domain::claim::{ClaimRecord, DueOrder, LaneKey, LaneOrder, ScopedRecordId};
use wf_domain::history::EventKind;
use wf_domain::lease::{Lease, LeaseToken};
use wf_domain::lineage::{
    ChildFailure, ChildGroup, ChildMember, ChildResolution, ChildStatus, ChildSummary, ChildWait,
    ParentClosed, ParentLink,
};
use wf_domain::policy::RetryOverride;
use wf_domain::policy::definition::{PolicySnapshot, StateMode, StatePolicy};
use wf_domain::policy::installation::PolicyLimits;
use wf_domain::policy::restoration::{PolicySnapshotData, restore_candidate};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::active::{
    ActiveCarry, ActiveIndexKey, ActiveRecord, ActiveRecordData, ActiveRecordWrite,
    ActiveTimeoutDecision, ActiveTimeoutPlan, ActiveTimeoutRejection, ActiveTimeoutView,
    ActiveValueRef, ActiveValueRefs, CapturedLifecycle, GovernanceReservation, HistoryKey,
    LifecycleDefaults, LifecycleOverrides, LimitEnforcement, MaxActiveOverride, ReleaseOutboxState,
    TimeoutLimits, active_rows, capture_lifecycle, expire_active_batch, restore_active,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::{ApplyError, GroupView};
use wf_domain::workflow::indexed::{EntryOrder, IndexedGroupView, OrderingMetadata};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_domain::workflow::{MAX_EXACT_MILLIS, WorkflowStatus};
use wf_types::identity::{GroupId, PartitionId, RecordId};
use wf_types::version::{AppliedPosition, Fence, LogIdentity, TimestampMillis};

mod authority;
mod capture;
mod checks;
mod closure;
mod fixtures;
mod model;

type TestError = Box<dyn std::error::Error>;
type TestResult<T = ()> = Result<T, TestError>;
type TestHistory = BTreeMap<(Vec<u8>, Vec<u8>), BTreeSet<(TimestampMillis, Vec<u8>)>>;

fn fixture(
    id: &[u8],
    partition: &[u8],
    state: &[u8],
    created: TimestampMillis,
    next: Option<TimestampMillis>,
    captured: CapturedLifecycle,
) -> TestResult<ActiveRecordData> {
    let lease = if state == b"running" {
        next.map(|deadline| {
            Ok::<_, TestError>(Lease {
                worker: b"worker".to_vec(),
                token: LeaseToken::new(b"token".to_vec())?,
                deadline,
            })
        })
        .transpose()?
    } else {
        None
    };
    let core = restore_record(
        WorkflowRecordData {
            id: RecordId::new(id.to_vec())?,
            workflow_type: b"job".to_vec(),
            state: state.to_vec(),
            version: 1,
            fence: Fence::new(1),
            attempts: 0,
            created_at: created,
            updated_at: created,
            next_run_at: next,
            run_state: None,
            lease,
            payload: None,
            result: None,
            error: None,
            latest_kind: EventKind::Created,
            latest_version: 1,
            latest_fence: Fence::new(1),
        },
        AdmissionLimits::default(),
    )?;
    let terminal = matches!(state, b"completed" | b"failed" | b"cancelled");
    Ok(ActiveRecordData {
        core,
        partition: PartitionId::new(partition.to_vec())?,
        ordering: OrderingMetadata {
            priority: 0,
            state_enter_seq: u128::from(created.get()),
        },
        captured,
        ttl_ms: None,
        terminal_retention_until: terminal.then(|| {
            TimestampMillis::new(
                created
                    .get()
                    .saturating_add(captured.retention_ttl_ms())
                    .min(MAX_EXACT_MILLIS),
            )
        }),
        parent: None,
        root_id: None,
        correlation_id: None,
        child_groups: BTreeMap::new(),
        values: ActiveValueRefs {
            payload: None,
            result: None,
            error: None,
        },
        carry: ActiveCarry {
            rewound_to_event_id: None,
            named_refs_history: None,
            attributes_history: None,
            state_meta_history: None,
            record_extensions: Vec::new(),
        },
        governance: None,
    })
}

fn with_view<T>(
    records: &[ActiveRecord],
    policies: &BTreeMap<Vec<u8>, PolicySnapshot>,
    history: &TestHistory,
    outbox: ReleaseOutboxState,
    body: impl for<'v> FnOnce(ActiveTimeoutView<'v>) -> TestResult<T>,
) -> TestResult<T> {
    let group = GroupId::new(b"g".to_vec())?;
    let mut placements = BTreeMap::new();
    let mut rich = BTreeMap::new();
    let mut cores = BTreeMap::new();
    let mut ordered = BTreeMap::new();
    let mut parallel = BTreeMap::new();
    let mut fifo = BTreeMap::<_, BTreeMap<_, _>>::new();
    let mut lifecycle = BTreeMap::new();
    let mut keys = BTreeMap::new();
    let policy_refs: BTreeMap<_, _> = policies
        .iter()
        .map(|(key, value)| (key.as_slice(), value))
        .collect();
    for record in records {
        let id = ScopedRecordId {
            partition_key: record.partition().as_bytes(),
            id: record.core().id().as_bytes(),
        };
        placements.insert(record.partition().clone(), group.clone());
        rich.insert(id, record);
        cores.insert(id, record.core());
        let metadata = record.ordering();
        let entry = ClaimRecord {
            record: record.core(),
            partition_key: id.partition_key,
            priority: metadata.priority,
            state_enter_seq: metadata.state_enter_seq,
        };
        ordered.insert(id, entry);
        let mode = policies
            .get(record.core().workflow_type())
            .map_or(StateMode::Parallel, |policy| {
                policy.state_mode(entry.logical_state())
            });
        if mode == StateMode::Fifo {
            if let Some(lane) = LaneKey::for_record(&entry) {
                fifo.entry(lane)
                    .or_default()
                    .insert(LaneOrder::for_record(&entry), entry);
            }
        } else if let Some(due) = DueOrder::for_record(&entry) {
            parallel.insert(due, entry);
        }
        if let Ok(rows) = active_rows(record, mode) {
            for (key, score) in rows.semantic.into_iter().flatten() {
                lifecycle.insert(key, score);
            }
        }
        let supplied = history.get(&(id.partition_key.to_vec(), id.id.to_vec()));
        keys.insert(
            id,
            supplied
                .into_iter()
                .flatten()
                .map(|(event_ms, event_id)| HistoryKey {
                    event_ms: *event_ms,
                    event_id,
                })
                .collect(),
        );
    }
    body(ActiveTimeoutView {
        indexed: IndexedGroupView {
            batch: GroupView {
                group: &group,
                placements: &placements,
                records: &cores,
            },
            records: &ordered,
            parallel: &parallel,
            fifo: &fifo,
            policies: &policy_refs,
        },
        records: &rich,
        history: &keys,
        lifecycle: &lifecycle,
        outbox: &outbox,
        release_shard: 0,
    })
}

fn expire_at<'v>(
    view: ActiveTimeoutView<'v>,
    roots: &[ScopedRecordId<'_>],
    now: TimestampMillis,
    order: EntryOrder,
    profiles: (AdmissionLimits, TimeoutLimits),
) -> Result<ActiveTimeoutDecision<'v>, ApplyError> {
    expire_active_batch(view, roots, now, order, profiles.0, profiles.1)
}

fn assert_no_plan(
    actual: Result<ActiveTimeoutDecision<'_>, ApplyError>,
    expected: Result<ActiveTimeoutRejection, ApplyError>,
) -> TestResult {
    match (actual, expected) {
        (Ok(ActiveTimeoutDecision::Rejected(actual)), Ok(expected)) => assert_eq!(actual, expected),
        (Err(actual), Err(expected)) => assert_eq!(actual, expected),
        _ => return Err("expected exactly the rejection and no plan".into()),
    }
    Ok(())
}

fn assert_history(
    plan: &ActiveTimeoutPlan<'_>,
    expected: &[(EventKind, u64, &[u8], TimestampMillis)],
) {
    assert_eq!(plan.history().len(), expected.len());
    for (actual, (kind, version, state, at)) in plan.history().iter().zip(expected) {
        assert_eq!(
            (
                actual.kind,
                actual.version,
                actual.state.as_slice(),
                actual.occurred_at
            ),
            (*kind, *version, *state, *at)
        );
    }
}

fn apply_timeout_plan(
    mut records: Vec<ActiveRecord>,
    writes: Vec<ActiveRecordWrite>,
) -> TestResult<Vec<ActiveRecord>> {
    for write in writes {
        let position = records
            .iter()
            .position(|old| {
                old.partition() == write.record.partition()
                    && old.core().id() == write.record.core().id()
            })
            .ok_or("unknown touched model record")?;
        assert_eq!(records[position].core().version(), write.expected_version);
        records[position] = write.record;
    }
    Ok(records)
}
