//! Real database inputs and literal output assertions, never catalog seeding.

use std::path::Path;

use redb::{ReadableDatabase, ReadableTable};
use sha2::{Digest, Sha256};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::active::{
    ActiveValueKind, ActiveValueRef, LifecycleDefaults, LifecycleOverrides, MaxActiveOverride,
    capture_lifecycle,
};
use wf_domain::workflow::control::PreparedDigest;
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::{CommittedCommand, Operation};
use wf_ports::blob::{
    WorkflowValueBindingKnowledge, WorkflowValueReadBatch, WorkflowValueReadLimits,
};
use wf_ports::clock::WorkflowValueReadClock;
use wf_ports::consensus::{
    PreparedBatchItem, PreparedOperation, PreparedWorkflowValueBatch, WorkflowValueDefaults,
    WorkflowValueItemInput, WorkflowValuePolicyWitness, WorkflowValueTargetGuard,
};
use wf_ports::log::encode_workflow_value_batch;
use wf_ports::state::StateTransactionError;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::{
    RecordBatchTransactionOutcome, WorkflowValueTransactionLimits,
    apply_workflow_value_batch_transaction,
};
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::{HlcTimestamp, TimestampMillis};

use super::TestResult;

mod policy;
pub(super) use policy::{
    complete_policy, install_complete, install_original, load_policy, original_policy, witness,
};

pub(super) struct ValueReadClock {
    pub(super) hlc: HlcTimestamp,
    pub(super) wall: TimestampMillis,
    pub(super) calls: usize,
    pub(super) failure: Option<StateTransactionError>,
}

impl WorkflowValueReadClock for ValueReadClock {
    fn read_snapshot(&mut self) -> Result<(HlcTimestamp, TimestampMillis), StateTransactionError> {
        self.calls = self
            .calls
            .checked_add(1)
            .ok_or(StateTransactionError::Limit)?;
        match self.failure {
            Some(error) => Err(error),
            None => Ok((self.hlc, self.wall)),
        }
    }
}

pub(super) fn defaults() -> WorkflowValueDefaults {
    let lifecycle = LifecycleDefaults {
        ttl_ms: 7_000,
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

pub(super) fn limits() -> WorkflowValueTransactionLimits {
    WorkflowValueTransactionLimits {
        record_batch: crate::fixtures::driver_limits(),
        maximum_value_rows: 1_024,
        maximum_value_bytes: 64 * crate::fixtures::MIB,
        maximum_value_work: 64 * crate::fixtures::MIB,
        maximum_value_temporary_bytes: 64 * crate::fixtures::MIB,
    }
}

pub(super) fn read_limits() -> WorkflowValueReadLimits {
    WorkflowValueReadLimits {
        maximum_references: 1_024,
        maximum_stored_bytes: 64 * crate::fixtures::MIB,
        maximum_work: 64 * crate::fixtures::MIB,
        maximum_temporary_bytes: 64 * crate::fixtures::MIB,
    }
}

pub(super) fn fresh(path: &Path) -> TestResult<Result<RedbStateStore, StateTransactionError>> {
    let scope = crate::fixtures::scope()?;
    let placements = crate::fixtures::placements()?;
    let configured = defaults();
    // This OLD scalar API validates the fixture input only. Its returned capture
    // is never used to calculate a new lifecycle expectation.
    assert!(
        capture_lifecycle(
            None,
            b"r",
            b"q",
            LifecycleOverrides {
                active: MaxActiveOverride::Inherit,
                retention: RetentionOverride::default(),
                history_hot_max_events: None,
            },
            configured.lifecycle,
        )
        .is_ok()
    );
    let database = redb::Builder::new().create(path)?;
    assert_eq!(database.begin_read()?.list_tables()?.count(), 0);
    Ok(RedbStateStore::create_value_database(
        database,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: crate::fixtures::clock(0)?,
            placements: &placements,
            policies: &[],
        },
        configured,
        StateStoreLimits::default(),
    ))
}

pub(super) fn batch(
    profile: u8,
    request: &[u8],
    now: u64,
    operation: PreparedOperation,
    inputs: Vec<WorkflowValueItemInput>,
    policies: Vec<WorkflowValuePolicyWitness>,
) -> TestResult<PreparedWorkflowValueBatch> {
    Ok(PreparedWorkflowValueBatch {
        scope: crate::fixtures::scope()?,
        request: RequestId::new(request.to_vec())?,
        stamp: crate::fixtures::clock(now)?,
        profile,
        policy_references_captured: Some(true),
        defaults: defaults(),
        policies,
        operation,
        inputs,
    })
}

pub(super) fn item(id: &[u8], now: u64, operation: Operation) -> TestResult<PreparedBatchItem> {
    Ok(PreparedBatchItem {
        partition: PartitionId::new(b"p".to_vec())?,
        command: CommittedCommand {
            id: RecordId::new(id.to_vec())?,
            now: TimestampMillis::new(now),
            operation,
        },
        create_priority: 0,
    })
}

pub(super) fn input(source_index: u32, policy_index: u32) -> WorkflowValueItemInput {
    WorkflowValueItemInput {
        source_index,
        policy_index,
        target: WorkflowValueTargetGuard::Unspecified,
        birth: None,
        terminal_ttl_ms: None,
        requested_at: None,
    }
}

pub(super) fn apply_value(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    batch: &PreparedWorkflowValueBatch,
    at: u64,
) -> TestResult<RecordBatchTransactionOutcome> {
    let encoded = encode_workflow_value_batch(batch, limits().record_batch.codec);
    assert!(encoded.is_ok(), "{:?}", encoded.as_ref().err());
    let Ok(encoded) = encoded else {
        return Err("expected positive value encoder".into());
    };
    let applied = apply_workflow_value_batch_transaction(
        store,
        scope,
        encoded.as_bytes(),
        crate::fixtures::log(at),
        limits(),
    );
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    let Ok(applied) = applied else {
        return Err("expected positive consumed value transaction".into());
    };
    Ok(applied)
}

pub(super) fn assert_binding(
    knowledge: &WorkflowValueBindingKnowledge,
    owner: (&[u8], &[u8]),
    incarnation: u128,
    versions: [Option<u64>; 3],
    lifecycle: (Option<u64>, u64, u32, u32, Option<u64>, Option<u64>),
    policy: &WorkflowValuePolicyWitness,
) -> TestResult {
    let WorkflowValueBindingKnowledge::Known(Some(binding)) = knowledge else {
        return Err("expected known present binding".into());
    };
    for (ordinal, (actual, expected)) in [
        binding.refs.payload.as_ref(),
        binding.refs.result.as_ref(),
        binding.refs.error.as_ref(),
    ]
    .into_iter()
    .zip(versions)
    .enumerate()
    {
        match (actual, expected) {
            (None, None) => {}
            (
                Some(ActiveValueRef::Versioned {
                    partition,
                    id,
                    kind,
                    version,
                }),
                Some(expected),
            ) => {
                assert_eq!(partition.as_bytes(), owner.0);
                assert_eq!(id.as_bytes(), owner.1);
                assert_eq!(*version, expected);
                assert!(matches!(
                    (ordinal, *kind),
                    (0, ActiveValueKind::Payload)
                        | (1, ActiveValueKind::Result)
                        | (2, ActiveValueKind::Error)
                ));
            }
            _ => return Err("unexpected reference class or presence".into()),
        }
    }
    let actual = &binding.lifecycle;
    assert_eq!(actual.incarnation, incarnation);
    assert_eq!(
        (
            actual.birth.max_active_ms(),
            actual.birth.retention_ttl_ms(),
            actual.birth.history_hot_max_events(),
            actual.birth.history_max_events(),
            actual.effective_ttl_ms,
            actual.terminal_deadline.map(TimestampMillis::get),
        ),
        lifecycle,
    );
    assert_eq!(actual.configuration_digest, defaults().configuration_digest);
    assert_eq!(actual.policy_profile, policy.profile);
    assert_eq!(actual.policy_generation, policy.generation);
    assert_eq!(actual.policy_digest, policy.digest);
    Ok(())
}

pub(super) fn assert_read(
    batch: &WorkflowValueReadBatch,
    values: &[&[u8]],
    slots: &[Option<usize>],
) -> TestResult {
    assert_eq!(batch.values.len(), values.len());
    assert_eq!(batch.slots.len(), slots.len());
    for (actual, expected) in batch.values.iter().zip(values) {
        assert_eq!(actual.as_slice(), *expected);
    }
    for (actual, expected) in batch.slots.iter().zip(slots) {
        match actual {
            Ok(index) => assert_eq!(*index, *expected),
            Err(error) => return Err(format!("unexpected read failure: {error:?}").into()),
        }
    }
    Ok(())
}

pub(super) fn raw_manifest(path: &Path) -> TestResult<Vec<u8>> {
    let database = redb::Builder::new().open(path)?;
    let read = database.begin_read()?;
    let table = read.open_table(crate::fixtures::META)?;
    let keys = table
        .iter()?
        .map(|row| Ok(row?.0.value().to_vec()))
        .collect::<TestResult<Vec<_>>>()?;
    assert_eq!(
        keys,
        [
            b"manifest".to_vec(),
            b"membership".to_vec(),
            b"origin".to_vec(),
            b"snapshot".to_vec(),
        ],
    );
    let manifest = table
        .get(b"manifest".as_slice())?
        .ok_or("manifest missing")?
        .value()
        .to_vec();
    Ok(manifest)
}
