//! Real value genesis, direct reference drivers and independent durable oracles.

use std::path::Path;

use redb::{ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use sha2::{Digest, Sha256};
use wf_domain::workflow::active::{ActiveValueKind, ActiveValueRef, LifecycleDefaults};
use wf_domain::workflow::caller::PublicMutationEvidence;
use wf_domain::workflow::control::{ControlState, PreparedDigest};
use wf_domain::workflow::request::{RequestScope, RetainedOutcome, RetainedStatus};
use wf_domain::workflow::{CommittedCommand, Create, Operation};
use wf_ports::blob::{
    OriginalValuePartition, OriginalValueRefKind, WorkflowValueBindingKnowledge,
    WorkflowValueReadBatch, WorkflowValueReadLimits, render_original_value_ref,
};
use wf_ports::clock::WorkflowValueReadClock;
use wf_ports::consensus::{
    PreparedBatchItem, PreparedOperation, PreparedWorkflowValueBatch, RaftMembershipLimits,
    WorkflowValueDefaults, WorkflowValueItemInput, WorkflowValuePolicyWitness,
    WorkflowValueTargetGuard,
};
use wf_ports::log::{PreparedBytes, RaftLogLimits, encode_workflow_value_batch};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError};
use wf_storage::log::RedbLogStore;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::{
    RecordBatchTransactionOutcome, WorkflowValueTransactionLimits,
    apply_workflow_value_batch_transaction,
};
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::{HlcTimestamp, TimestampMillis};

use super::{Error, Limits, TestResult, f, recover_committed};

mod policy;
pub(super) use policy::{install, policy_frame, witness};

pub(super) struct ValueReadClock {
    pub(super) hlc: HlcTimestamp,
    pub(super) wall: TimestampMillis,
}

impl WorkflowValueReadClock for ValueReadClock {
    fn read_snapshot(&mut self) -> Result<(HlcTimestamp, TimestampMillis), StateTransactionError> {
        Ok((self.hlc, self.wall))
    }
}

pub(super) fn defaults(ttl_ms: u64) -> WorkflowValueDefaults {
    let lifecycle = LifecycleDefaults {
        ttl_ms,
        history_hot_max_events: 2,
        history_max_events: 10,
        max_history_hot_max_events: 100,
        max_history_max_events: 1_000,
    };
    let mut hash = Sha256::new();
    hash.update(b"wf-workflow-value-defaults-v1");
    hash.update(lifecycle.ttl_ms.to_be_bytes());
    hash.update(lifecycle.history_hot_max_events.to_be_bytes());
    hash.update(lifecycle.history_max_events.to_be_bytes());
    hash.update(lifecycle.max_history_hot_max_events.to_be_bytes());
    hash.update(lifecycle.max_history_max_events.to_be_bytes());
    WorkflowValueDefaults {
        configuration_digest: PreparedDigest::new(hash.finalize().into()),
        lifecycle,
    }
}

pub(super) fn copy_policy(value: &WorkflowValuePolicyWitness) -> WorkflowValuePolicyWitness {
    WorkflowValuePolicyWitness {
        workflow_type: value.workflow_type.clone(),
        profile: value.profile,
        generation: value.generation,
        digest: value.digest,
    }
}

pub(super) fn limits() -> TestResult<Limits> {
    let mut normal = f::driver_limits();
    normal.workspace_bytes = 512 * f::MIB;
    Ok(Limits::new(
        RaftMembershipLimits::default(),
        (10_000, 64 * f::MIB, 64 * f::MIB, 32 * f::MIB),
        normal,
        (1_000, 64 * f::MIB),
        16 * f::MIB,
        1_024 * f::MIB,
    )?
    .with_core_policy_patches()
    .with_original_policy_sets())
}

pub(super) fn driver(limits: &Limits) -> WorkflowValueTransactionLimits {
    WorkflowValueTransactionLimits {
        record_batch: limits.normal_limits(),
        maximum_value_rows: 1_024,
        maximum_value_bytes: 64 * f::MIB,
        maximum_value_work: 64 * f::MIB,
        maximum_value_temporary_bytes: 64 * f::MIB,
    }
}

pub(super) fn read_limits() -> WorkflowValueReadLimits {
    WorkflowValueReadLimits {
        maximum_references: 1_024,
        maximum_stored_bytes: 64 * f::MIB,
        maximum_work: 64 * f::MIB,
        maximum_temporary_bytes: 64 * f::MIB,
    }
}

pub(super) fn log(dir: &Path) -> TestResult<RedbLogStore> {
    let scope = f::scope()?;
    Ok(RedbLogStore::create(
        &dir.join("log.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        RaftLogLimits::default(),
    )?)
}

pub(super) fn pair(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore)> {
    pair_for(dir, &f::scope()?)
}

pub(super) fn pair_for(
    dir: &Path,
    scope: &RequestScope,
) -> TestResult<(RedbLogStore, RedbStateStore)> {
    let database = redb::Builder::new().create(dir.join("state.redb"))?;
    assert_eq!(database.begin_read()?.list_tables()?.count(), 0);
    let placements = [(PartitionId::new(b"p".to_vec())?, scope.group().clone())];
    let state = RedbStateStore::create_value_database(
        database,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0, 0, 0)?,
            placements: &placements,
            policies: &[],
        },
        defaults(7_000),
        StateStoreLimits::default(),
    )?;
    let log = RedbLogStore::create(
        &dir.join("log.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        RaftLogLimits::default(),
    )?;
    Ok((log, state))
}

pub(super) fn item(id: &[u8], now: u64, operation: Operation) -> TestResult<PreparedBatchItem> {
    Ok(PreparedBatchItem {
        partition: PartitionId::new(b"p".to_vec())?,
        create_priority: 0,
        command: CommittedCommand {
            id: RecordId::new(id.to_vec())?,
            now: TimestampMillis::new(now),
            operation,
        },
    })
}

pub(super) fn create(id: &[u8], now: u64, payload: &[u8]) -> TestResult<PreparedBatchItem> {
    item(
        id,
        now,
        Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: Some(payload.to_vec()),
        }),
    )
}

pub(super) fn batch(
    scope: &RequestScope,
    mode: u8,
    request: &[u8],
    now: u64,
    mut items: Vec<PreparedBatchItem>,
    policy: WorkflowValuePolicyWitness,
) -> TestResult<PreparedWorkflowValueBatch> {
    let inputs = (0..items.len())
        .map(|index| {
            Ok(WorkflowValueItemInput {
                source_index: u32::try_from(index)?,
                policy_index: 0,
                target: WorkflowValueTargetGuard::Unspecified,
                birth: None,
                terminal_ttl_ms: None,
                requested_at: None,
            })
        })
        .collect::<TestResult<Vec<_>>>()?;
    let operation = if mode == 1 {
        assert_eq!(items.len(), 1);
        PreparedOperation::PublicMutation {
            item: Box::new(items.pop().ok_or("public item missing")?),
            evidence: PublicMutationEvidence {
                requested_now_ms: None,
                auto_partition: false,
                retry_override_present: false,
            },
        }
    } else {
        PreparedOperation::Batch(items)
    };
    Ok(PreparedWorkflowValueBatch {
        scope: scope.clone(),
        request: RequestId::new(request.to_vec())?,
        stamp: f::clock(now, 0, now)?,
        profile: mode,
        policy_references_captured: Some(true),
        defaults: defaults(7_000),
        policies: vec![policy],
        operation,
        inputs,
    })
}

pub(super) fn encode(batch: &PreparedWorkflowValueBatch) -> TestResult<PreparedBytes> {
    Ok(encode_workflow_value_batch(
        batch,
        limits()?.normal_limits().codec,
    )?)
}

pub(super) fn apply(
    state: &mut RedbStateStore,
    frame: &PreparedBytes,
    at: u64,
) -> TestResult<RecordBatchTransactionOutcome> {
    let result = apply_workflow_value_batch_transaction(
        state,
        &f::scope()?,
        frame.as_bytes(),
        f::at(at),
        driver(&limits()?),
    )?;
    Ok(result)
}

pub(super) fn accepted(
    outcome: RecordBatchTransactionOutcome,
    at: u64,
    now: u64,
    mode: u8,
) -> TestResult {
    let RecordBatchTransactionOutcome::Retained { control, outcome } = outcome else {
        return Err("expected actual retained acceptance".into());
    };
    assert_eq!(control.position(), f::at(at).index());
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.intent().version(), 8);
    assert_eq!(
        outcome.response_version(),
        if mode == 1 { 8 } else { u16::from(mode) }
    );
    assert_eq!(outcome.original_position(), f::at(at));
    assert_eq!(outcome.original_time().get(), now);
    if mode == 5 {
        assert_eq!(outcome.response(), [0]);
    } else {
        assert_ne!(outcome.response(), []);
    }
    Ok(())
}

pub(super) fn outcome(state: &mut RedbStateStore, request: &[u8]) -> TestResult<RetainedOutcome> {
    let scope = f::scope()?;
    let limits = limits()?.normal_limits();
    let tx = state.begin_apply(
        scope.group(),
        scope.recovery(),
        limits.codec.admission(),
        limits.maximum_read_steps,
        limits.maximum_read_bytes,
        limits.maximum_adapter_bytes,
    )?;
    let outcome = tx
        .outcome(scope.tenant(), &RequestId::new(request.to_vec())?)?
        .ok_or("retained outcome missing")?;
    drop(tx);
    Ok(outcome)
}

pub(super) fn cohort(dir: &Path) -> TestResult<[f::Rows; 6]> {
    let database = redb::Builder::new().open(dir.join("state.redb"))?;
    let read = database.begin_read()?;
    let names = [
        "wf_state_meta_v1",
        "wf_state_rows_v1",
        "wf_state_snapshot_meta_v1",
        "wf_state_snapshot_rows_v1",
        "wf_state_snapshot_alt_meta_v1",
        "wf_state_snapshot_alt_rows_v1",
    ];
    let mut actual = read
        .list_tables()?
        .map(|table| table.name().to_owned())
        .collect::<Vec<_>>();
    actual.sort();
    let mut expected = names.map(str::to_owned);
    expected.sort();
    assert_eq!(actual, expected);
    assert_eq!(read.list_multimap_tables()?.count(), 0);
    let mut result = std::array::from_fn(|_| f::Rows::new());
    for (rows, name) in result.iter_mut().zip(names) {
        let table = read.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
        for row in table.iter()? {
            let (key, value) = row?;
            assert!(
                rows.insert(key.value().to_vec(), value.value().to_vec())
                    .is_none()
            );
        }
    }
    assert_eq!(
        result[0].keys().map(Vec::as_slice).collect::<Vec<_>>(),
        [
            b"manifest".as_slice(),
            b"membership",
            b"origin",
            b"snapshot"
        ]
    );
    assert!(result[0][b"manifest".as_slice()].starts_with(b"FWSS\0\x07"));
    assert!(result[0][b"snapshot".as_slice()].starts_with(b"FWSD\0\x03"));
    Ok(result)
}

pub(super) fn refused(dir: &Path, limits: &Limits, expected: Error) -> TestResult {
    let before = cohort(dir)?;
    let rows = f::log_rows(dir)?;
    let meta = f::log_meta(dir)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(dir)?;
    let error = recover_committed(log, state, scope.group(), scope.recovery(), *limits)
        .err()
        .ok_or("refused Node value recovery succeeded")?;
    assert_eq!(error, expected);
    drop(f::open_pair(dir)?);
    assert_eq!(cohort(dir)?, before);
    assert_eq!(f::log_rows(dir)?, rows);
    assert_eq!(f::log_meta(dir)?, meta);
    Ok(())
}

pub(super) fn reference(
    id: &[u8],
    kind: OriginalValueRefKind,
    version: u64,
) -> TestResult<Vec<u8>> {
    render_original_value_ref(
        id,
        kind,
        version,
        OriginalValuePartition::Explicit(b"p"),
        65_535,
    )
    .map_err(|error| format!("literal reference input: {error:?}").into())
}

pub(super) fn read(
    state: &RedbStateStore,
    references: &[&[u8]],
) -> TestResult<WorkflowValueReadBatch> {
    let mut clock = ValueReadClock {
        hlc: HlcTimestamp::new(500, 0)?,
        wall: TimestampMillis::new(500),
    };
    Ok(state.read_workflow_value_batch(&mut clock, &f::scope()?, references, read_limits())?)
}

pub(super) fn assert_read(
    read: &WorkflowValueReadBatch,
    values: &[&[u8]],
    slots: &[Option<usize>],
) {
    assert_eq!(read.values.len(), values.len());
    assert_eq!(read.slots.len(), slots.len());
    for (actual, expected) in read.values.iter().zip(values) {
        assert_eq!(actual.as_slice(), *expected);
    }
    for (actual, expected) in read.slots.iter().zip(slots) {
        assert!(matches!(actual, Ok(index) if index == expected));
    }
}

pub(super) fn binding(
    state: &RedbStateStore,
    id: &[u8],
    history: Option<&str>,
    expected: (u128, [Option<u64>; 3], Option<u64>),
    policy: &WorkflowValuePolicyWitness,
) -> TestResult {
    let value = state.workflow_value_binding(
        &f::scope()?,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(id.to_vec())?,
        history,
        read_limits(),
    )?;
    let WorkflowValueBindingKnowledge::Known(Some(binding)) = value else {
        return Err("missing complete current/history binding".into());
    };
    for ((actual, version), kind) in [
        binding.refs.payload,
        binding.refs.result,
        binding.refs.error,
    ]
    .into_iter()
    .zip(expected.1)
    .zip([
        ActiveValueKind::Payload,
        ActiveValueKind::Result,
        ActiveValueKind::Error,
    ]) {
        match (actual, version) {
            (None, None) => {}
            (
                Some(ActiveValueRef::Versioned {
                    partition,
                    id: actual,
                    kind: actual_kind,
                    version,
                }),
                Some(want),
            ) => {
                assert_eq!(partition.as_bytes(), b"p");
                assert_eq!(actual.as_bytes(), id);
                assert!(actual_kind == kind);
                assert_eq!(version, want);
            }
            _ => return Err("incorrect value reference presence or class".into()),
        }
    }
    let lifecycle = binding.lifecycle;
    assert_eq!(lifecycle.incarnation, expected.0);
    assert_eq!(lifecycle.birth.max_active_ms(), Some(5_000));
    assert_eq!(lifecycle.birth.retention_ttl_ms(), 22_000);
    assert_eq!(lifecycle.birth.history_hot_max_events(), 2);
    assert_eq!(lifecycle.birth.history_max_events(), 40);
    assert_eq!(lifecycle.effective_ttl_ms, Some(22_000));
    assert_eq!(
        lifecycle.terminal_deadline.map(TimestampMillis::get),
        expected.2
    );
    assert_eq!(
        lifecycle.configuration_digest,
        defaults(7_000).configuration_digest
    );
    assert_eq!(lifecycle.policy_profile, policy.profile);
    assert_eq!(lifecycle.policy_generation, policy.generation);
    assert_eq!(lifecycle.policy_digest, policy.digest);
    Ok(())
}

pub(super) fn control(state: &mut RedbStateStore) -> TestResult<ControlState> {
    Ok(f::applied(state)?.0)
}
