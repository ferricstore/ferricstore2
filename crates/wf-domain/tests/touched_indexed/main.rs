//! Explicit observed points; complete maps remain a separate comparison oracle.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::error::Error;

use wf_domain::claim::{Claim, ClaimRecord, DueOrder, LaneKey, LaneOrder, ScopedRecordId};
use wf_domain::history::EventKind;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::policy::definition::{
    FieldPatch, PolicyPatch, StateChange, StateMode, StatePolicyPatch,
};
use wf_domain::policy::installation::{
    InstallCommand, PolicyCandidate, PolicyLimits, prepare_install,
};
use wf_domain::policy::{InstalledRetryPolicy, RetryOverride};
use wf_domain::retry::{JitterSample, PolicyRetry};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::{ApplyError, BatchItem, BatchRejection, GroupView};
use wf_domain::workflow::indexed::{
    EntryOrder, IndexRejection, IndexRows, IndexedBatchItem, IndexedDecision, IndexedGroupView,
    TouchedIndexedGroupView, decide_indexed_batch, decide_touched_indexed_batch,
};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_domain::workflow::{
    Cancel, CommittedCommand, Create, Operation, Transition, WorkflowRecord, reduce,
};
use wf_types::identity::{GroupId, PartitionId, RecordId};
use wf_types::version::{AppliedPosition, Fence, LogIdentity, TimestampMillis};

mod authority;
mod mutations;

fn create(id: &[u8]) -> Result<CommittedCommand, Box<dyn Error>> {
    Ok(CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(10),
        operation: Operation::Create(Create {
            workflow_type: b"job".to_vec(),
            state: b"queued".to_vec(),
            run_at: None,
            payload: None,
        }),
    })
}

fn record(id: &[u8]) -> Result<WorkflowRecord, Box<dyn Error>> {
    Ok(reduce(None, &create(id)?)?
        .into_parts()
        .1
        .ok_or("write absent")?
        .record)
}

fn entry(record: &WorkflowRecord, sequence: u128) -> ClaimRecord<'_> {
    ClaimRecord {
        record,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: sequence,
    }
}

fn order(index: u64, first_ordinal: u32) -> EntryOrder {
    EntryOrder {
        identity: LogIdentity::new(3, 7, AppliedPosition::new(index)),
        first_ordinal,
    }
}

fn fifo_policy() -> Result<PolicyCandidate, Box<dyn Error>> {
    let states = [b"queued".as_slice(), b"next", b"waiting"]
        .into_iter()
        .map(|state| {
            (
                state.to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    mode: FieldPatch::Set(StateMode::Fifo),
                    ..StatePolicyPatch::default()
                }),
            )
        })
        .collect();
    Ok(prepare_install(
        None,
        0,
        &InstallCommand::low_level(
            b"job".to_vec(),
            PolicyPatch {
                states,
                ..PolicyPatch::default()
            },
        ),
        PolicyLimits::default(),
    )?)
}

fn with_views<'current, T>(
    entries: &[ClaimRecord<'current>],
    commands: &[&CommittedCommand],
    fifo_mode: bool,
    inspect: impl FnOnce(
        IndexedGroupView<'_>,
        TouchedIndexedGroupView<'_, 'current>,
    ) -> Result<T, Box<dyn Error>>,
) -> Result<T, Box<dyn Error>> {
    let group = GroupId::new(b"g".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let placements = BTreeMap::from([(partition, group.clone())]);
    let core = entries
        .iter()
        .map(|row| (ScopedRecordId::for_record(row), row.record))
        .collect();
    let canonical = entries
        .iter()
        .map(|row| (ScopedRecordId::for_record(row), *row))
        .collect();
    let parallel = entries
        .iter()
        .filter_map(|row| {
            (!fifo_mode)
                .then(|| DueOrder::for_record(row).map(|key| (key, *row)))
                .flatten()
        })
        .collect();
    let mut fifo: BTreeMap<_, BTreeMap<_, _>> = BTreeMap::new();
    for row in entries {
        if fifo_mode && let Some(lane) = LaneKey::for_record(row) {
            fifo.entry(lane)
                .or_default()
                .insert(LaneOrder::for_record(row), *row);
        }
    }
    let policy = fifo_policy()?;
    let policies = if fifo_mode {
        BTreeMap::from([(b"job".as_slice(), policy.policy())])
    } else {
        BTreeMap::new()
    };
    let full = IndexedGroupView {
        batch: GroupView {
            group: &group,
            placements: &placements,
            records: &core,
        },
        records: &canonical,
        parallel: &parallel,
        fifo: &fifo,
        policies: &policies,
    };
    with_facts(full, entries, commands, fifo_mode, inspect)
}

fn with_facts<'current, T>(
    full: IndexedGroupView<'_>,
    entries: &[ClaimRecord<'current>],
    commands: &[&CommittedCommand],
    fifo_mode: bool,
    inspect: impl FnOnce(
        IndexedGroupView<'_>,
        TouchedIndexedGroupView<'_, 'current>,
    ) -> Result<T, Box<dyn Error>>,
) -> Result<T, Box<dyn Error>> {
    let query_ids: Vec<_> = entries
        .iter()
        .map(|row| row.record.id().as_bytes().to_vec())
        .chain(
            commands
                .iter()
                .map(|command| command.id.as_bytes().to_vec()),
        )
        .collect();
    let observed: BTreeMap<_, Option<ClaimRecord<'current>>> = query_ids
        .iter()
        .map(|id| {
            (
                ScopedRecordId {
                    partition_key: b"p",
                    id,
                },
                entries
                    .iter()
                    .find(|row| row.record.id().as_bytes() == id)
                    .copied(),
            )
        })
        .collect();
    let due_facts = entries
        .iter()
        .filter_map(|row| DueOrder::for_record(row).map(|key| (key, !fifo_mode)))
        .collect();
    let members = entries
        .iter()
        .filter_map(|row| {
            LaneKey::for_record(row).map(|lane| {
                (
                    (lane, LaneOrder::for_record(row)),
                    fifo_mode.then_some(*row),
                )
            })
        })
        .collect();
    let heads = entries
        .iter()
        .filter_map(|row| {
            LaneKey::for_record(row).map(|lane| {
                (
                    lane,
                    if fifo_mode {
                        entries
                            .iter()
                            .filter(|row| LaneKey::for_record(row) == Some(lane))
                            .min_by_key(|row| LaneOrder::for_record(row))
                            .map(|row| (LaneOrder::for_record(row), *row))
                    } else {
                        None
                    },
                )
            })
        })
        .collect();
    let modes = BTreeMap::from([(
        b"job".as_slice(),
        full.policies.get(b"job".as_slice()).copied(),
    )]);
    let retries = BTreeMap::from([(b"job".as_slice(), None)]);
    inspect(
        full,
        TouchedIndexedGroupView {
            group: full.batch.group,
            placements: full.batch.placements,
            current: &observed,
            parallel: &due_facts,
            fifo_members: &members,
            fifo_heads: &heads,
            policies: &modes,
            retry_policies: &retries,
        },
    )
}

fn assert_invalid(view: TouchedIndexedGroupView<'_, '_>, command: &CommittedCommand) {
    assert_eq!(
        decide(view, &[command], order(2, 0)),
        Err(ApplyError::InvalidAuthority)
    );
}

fn decide<'current>(
    view: TouchedIndexedGroupView<'_, 'current>,
    commands: &[&CommittedCommand],
    order: EntryOrder,
) -> Result<IndexedDecision<'current>, ApplyError> {
    let partition = PartitionId::new(b"p".to_vec()).map_err(|_| ApplyError::InvalidAuthority)?;
    let items: Vec<_> = commands
        .iter()
        .map(|command| IndexedBatchItem::new(BatchItem::new(&partition, command, b"key")))
        .collect();
    decide_touched_indexed_batch(view, &items, order, b"entry", AdmissionLimits::default())
}

fn complete<'view>(
    view: IndexedGroupView<'view>,
    commands: &[&CommittedCommand],
    order: EntryOrder,
) -> Result<IndexedDecision<'view>, ApplyError> {
    let partition = PartitionId::new(b"p".to_vec()).map_err(|_| ApplyError::InvalidAuthority)?;
    let items: Vec<_> = commands
        .iter()
        .map(|command| IndexedBatchItem::new(BatchItem::new(&partition, command, b"key")))
        .collect();
    decide_indexed_batch(view, &items, order, b"entry", AdmissionLimits::default())
}

fn transition(id: &[u8]) -> Result<CommittedCommand, Box<dyn Error>> {
    let mut command = create(id)?;
    command.now = TimestampMillis::new(20);
    command.operation = Operation::Transition(Transition {
        from: b"queued".to_vec(),
        to: b"next".to_vec(),
        fence: Fence::new(0),
        lease_token: None,
        run_at: None,
        payload: Some(b"new".to_vec()),
    });
    Ok(command)
}

fn claim(id: &[u8]) -> Result<CommittedCommand, Box<dyn Error>> {
    let mut command = create(id)?;
    command.operation = Operation::Claim(Claim {
        expected_state: b"queued".to_vec(),
        worker: b"worker".to_vec(),
        token: LeaseToken::new(b"token".to_vec())?,
        lease_ms: 100,
    });
    Ok(command)
}

fn unscheduled(id: &[u8]) -> Result<WorkflowRecord, Box<dyn Error>> {
    Ok(restore_record(
        WorkflowRecordData {
            id: RecordId::new(id.to_vec())?,
            workflow_type: b"job".to_vec(),
            state: b"waiting".to_vec(),
            version: 1,
            fence: Fence::new(0),
            attempts: 0,
            created_at: TimestampMillis::new(10),
            updated_at: TimestampMillis::new(10),
            next_run_at: None,
            run_state: None,
            lease: None,
            payload: None,
            result: None,
            error: None,
            latest_kind: EventKind::Transitioned,
            latest_version: 1,
            latest_fence: Fence::new(0),
        },
        AdmissionLimits::default(),
    )?)
}
