//! Real native metadata genesis and consumed create/cancel fixture helpers.
use super::*;
use sha2::{Digest, Sha256};
use wf_application::queries::{StrongWorkflowRecordLimits, StrongWorkflowRecordRequest};
use wf_domain::{
    policy::complete::{
        OriginalPolicyReadValue, PolicySchemaCharge, PolicySchemaLimits,
        canonical_original_etf_safety,
    },
    value::{OriginalValueContext, OriginalValueLimits},
    workflow::{
        Cancel, CommittedCommand, Create, Operation, active::LifecycleDefaults,
        request::RequestScope,
    },
};
use wf_ports::{
    consensus::{
        PreparedBatchItem, PreparedOperation, PreparedWorkflowValueBatch, WorkflowValueDefaults,
        WorkflowValueItemInput, WorkflowValuePolicyWitness, WorkflowValueTargetGuard,
    },
    log::{
        MetadataBatchInput, MetadataBatchLimits, encode_workflow_metadata_batch,
        encode_workflow_stored_value_batch, prepare_original_complete_policy_reference,
    },
    projection::query_row::{NativeQueryPreparationLimits, OriginalQueryRecordLimits},
    state::{
        WorkflowRecordPolicyReadLimits, WorkflowRecordPolicyReadPort, WorkflowRecordReadLimits,
        WorkflowRecordReadQuery,
    },
};
use wf_storage::{
    state::{RedbStateStore, StateStoreGenesis, StateStoreLimits},
    transaction::{
        RecordBatchTransactionOutcome, WorkflowValueTransactionLimits,
        apply_workflow_metadata_batch_transaction,
    },
};
use wf_types::{
    identity::{PartitionId, RequestId},
    version::Fence,
};

pub(super) enum QueryFault {
    None,
    Deny,
    Unavailable,
    BarrierAhead,
    WrongIdentity,
    WrongPoint,
    EmptyControl,
}
pub(super) struct QueryFixture {
    pub(super) store: RedbStateStore,
    pub(super) directory: std::path::PathBuf,
    pub(super) now: TimestampMillis,
    pub(super) reads: usize,
    pub(super) fault: QueryFault,
    pub(super) active: Option<u64>,
    partition: PartitionId,
    prefix: Option<Vec<u8>>,
}
impl QueryFixture {
    pub(super) fn new(tag: &str) -> TestResult<Self> {
        Self::scoped(tag, None)
    }
    pub(super) fn scoped(tag: &str, prefix: Option<&[u8]>) -> TestResult<Self> {
        let directory = f::directory(tag)?;
        let scope = f::scope()?;
        let physical = wf_domain::metadata::scope::original_physical_partition(
            b"p",
            prefix,
            wf_domain::metadata::scope::OriginalScopeLimits {
                maximum_output_bytes: 65536,
                maximum_work_units: MIB,
                maximum_temporary_bytes: MIB,
            },
        )
        .map_err(|e| format!("routing: {e:?}"))?
        .value
        .into_owned();
        let partition = PartitionId::new(physical)?;
        let placements = [(partition.clone(), scope.group().clone())];
        let store = RedbStateStore::create_metadata_value_database(
            redb::Builder::new().create(directory.join("state.redb"))?,
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreGenesis {
                initial_time: f::clock(0, 0, 0)?,
                placements: &placements,
                policies: &[],
            },
            defaults(),
            StateStoreLimits::default(),
        )?;
        Ok(Self {
            store,
            directory,
            now: TimestampMillis::new(100),
            reads: 0,
            fault: QueryFault::None,
            active: None,
            partition,
            prefix: prefix.map(<[u8]>::to_vec),
        })
    }
    pub(super) fn create(&mut self) -> TestResult {
        self.apply(
            Operation::Create(Create {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                run_at: None,
                payload: Some(b"data".to_vec()),
            }),
            0,
            100,
        )
    }
    pub(super) fn cancel(&mut self) -> TestResult {
        self.apply(
            Operation::Cancel(Cancel {
                fence: Fence::new(0),
                lease_token: None,
                expected_state: None,
                expected_version: None,
                error: None,
            }),
            1,
            110,
        )
    }
    fn apply(&mut self, operation: Operation, at: u64, now: u64) -> TestResult {
        let scope = f::scope()?;
        let target = if at == 0 {
            WorkflowValueTargetGuard::Absent
        } else {
            let snapshot = self.store.read_workflow_record_with_policy(
                &scope,
                &self.partition,
                &RecordId::new(b"i".to_vec())?,
                WorkflowRecordReadQuery::default(),
                limits().read,
            )?;
            WorkflowValueTargetGuard::Present {
                incarnation: snapshot
                    .record
                    .capture
                    .ok_or("birth")?
                    .values
                    .lifecycle
                    .incarnation,
            }
        };
        let batch = PreparedWorkflowValueBatch {
            scope: scope.clone(),
            request: RequestId::new(format!("query-{at}").into_bytes())?,
            stamp: f::clock(now, 0, now)?,
            profile: 5,
            policy_references_captured: Some(true),
            defaults: defaults(),
            policies: vec![witness()?],
            operation: PreparedOperation::Batch(vec![PreparedBatchItem {
                partition: self.partition.clone(),
                command: CommittedCommand {
                    id: RecordId::new(b"i".to_vec())?,
                    now: TimestampMillis::new(now),
                    operation,
                },
                create_priority: 0,
            }]),
            inputs: vec![WorkflowValueItemInput {
                source_index: 0,
                policy_index: 0,
                target,
                birth: self.active.filter(|_| at == 0).map(|duration| {
                    wf_domain::workflow::active::LifecycleOverrides {
                        active: wf_domain::workflow::active::MaxActiveOverride::Millis(duration),
                        retention: wf_domain::retention::RetentionOverride::default(),
                        history_hot_max_events: None,
                    }
                }),
                terminal_ttl_ms: None,
                requested_at: None,
            }],
        };
        let driver = WorkflowValueTransactionLimits {
            record_batch: driver_limits(),
            maximum_value_rows: 1024,
            maximum_value_bytes: 64 * f::MIB,
            maximum_value_work: 64 * f::MIB,
            maximum_value_temporary_bytes: 64 * f::MIB,
        };
        let inner = encode_workflow_stored_value_batch(&batch, driver.record_batch.codec)?;
        let metadata = metadata(self.prefix.as_deref())?;
        let values = [if at == 0 {
            Some(metadata.as_slice())
        } else {
            None
        }];
        let bytes = encode_workflow_metadata_batch(
            MetadataBatchInput {
                stored_batch: inner.as_bytes(),
                metadata: &values,
            },
            MetadataBatchLimits {
                codec: driver.record_batch.codec,
                maximum_owned_bytes: 64 * f::MIB,
                maximum_work_units: 64 * f::MIB,
            },
        )?;
        let outcome = apply_workflow_metadata_batch_transaction(
            &mut self.store,
            &scope,
            bytes.as_bytes(),
            f::at(at),
            driver,
        )?;
        let RecordBatchTransactionOutcome::Retained { outcome, .. } = outcome else {
            return Err("actual retained outcome".into());
        };
        assert_eq!(
            outcome.status(),
            wf_domain::workflow::request::RetainedStatus::Accepted
        );
        Ok(())
    }
    pub(super) fn finish(self) -> TestResult {
        let directory = self.directory;
        drop(self.store);
        std::fs::remove_dir_all(directory)?;
        Ok(())
    }
}
fn defaults() -> WorkflowValueDefaults {
    let lifecycle = LifecycleDefaults {
        ttl_ms: 10,
        history_hot_max_events: 2,
        history_max_events: 10,
        max_history_hot_max_events: 100,
        max_history_max_events: 1000,
    };
    let mut hash = Sha256::new();
    hash.update(b"wf-workflow-value-defaults-v1");
    hash.update(lifecycle.ttl_ms.to_be_bytes());
    hash.update(lifecycle.history_hot_max_events.to_be_bytes());
    hash.update(lifecycle.history_max_events.to_be_bytes());
    hash.update(lifecycle.max_history_hot_max_events.to_be_bytes());
    hash.update(lifecycle.max_history_max_events.to_be_bytes());
    WorkflowValueDefaults {
        configuration_digest: wf_domain::workflow::control::PreparedDigest::new(
            hash.finalize().into(),
        ),
        lifecycle,
    }
}
pub(super) fn context() -> OriginalValueContext<'static> {
    OriginalValueContext {
        safe_atoms: Some(&["nil"]),
        active_exports: None,
        local_node: None,
        local_functions: None,
    }
}
pub(super) fn limits() -> StrongWorkflowRecordLimits {
    StrongWorkflowRecordLimits {
        maximum_input_bytes: f::MIB,
        read: WorkflowRecordPolicyReadLimits {
            record: WorkflowRecordReadLimits {
                maximum_row_reads: 16,
                maximum_inspected_bytes: 8 * f::MIB,
                maximum_returned_bytes: 8 * f::MIB,
                maximum_work: 64 * f::MIB,
                maximum_temporary_bytes: 32 * f::MIB,
            },
            schema: PolicySchemaLimits::default(),
        },
        preparation: NativeQueryPreparationLimits {
            query: OriginalQueryRecordLimits {
                maximum_output_bytes: 64 * f::MIB,
                maximum_work_units: 256 * f::MIB,
                maximum_temporary_bytes: 64 * f::MIB,
            },
            metadata: OriginalValueLimits {
                max_encoded_bytes: f::MIB,
                max_nodes: 10000,
                max_edges: 20000,
                max_depth: 64,
                max_work_units: 64 * f::MIB,
            },
            policy: PolicySchemaLimits::default(),
        },
    }
}
pub(super) fn request<'a>(
    scope: &'a RequestScope,
    id: &'a RecordId,
    after: Option<WorkflowReadPosition<'a>>,
) -> StrongWorkflowRecordRequest<'a> {
    StrongWorkflowRecordRequest {
        scope,
        id,
        partition: b"p",
        scope_prefix: None,
        after,
    }
}
fn metadata(prefix: Option<&[u8]>) -> TestResult<Vec<u8>> {
    let mut bytes = b"FSV2\x02\x83\x68\x03".to_vec();
    binary(&mut bytes, b"wf.record.metadata")?;
    bytes.extend_from_slice(&[97, 1, 116, 0, 0, 0, 2]);
    binary(&mut bytes, b"partition_key")?;
    binary(&mut bytes, b"p")?;
    binary(&mut bytes, b"scope_prefix")?;
    match prefix {
        Some(value) => binary(&mut bytes, value)?,
        None => bytes.extend_from_slice(b"\x77\x03nil"),
    }
    Ok(bytes)
}
fn binary(out: &mut Vec<u8>, value: &[u8]) -> TestResult {
    out.push(109);
    out.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}

pub(super) const MIB: usize = 1_048_576;
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub(super) fn directory(label: &str) -> TestResult<std::path::PathBuf> {
    let path = std::env::temp_dir().join(format!(
        "wf-query-{}-{}-{label}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    std::fs::create_dir(&path)?;
    Ok(path)
}
pub(super) fn scope() -> TestResult<RequestScope> {
    scope_for(b"t", b"g", b"z")
}
pub(super) fn scope_for(tenant: &[u8], group: &[u8], recovery: &[u8]) -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        wf_types::identity::TenantId::new(tenant.to_vec())?,
        wf_types::identity::GroupId::new(group.to_vec())?,
        wf_types::identity::RecoveryId::new(recovery.to_vec())?,
    ))
}
pub(super) fn clock(
    physical: u64,
    logical: u16,
    wall: u64,
) -> TestResult<wf_types::version::CommittedTime> {
    Ok(wf_types::version::CommittedTime::new(
        wf_types::version::HlcTimestamp::new(physical, logical)?,
        wall,
    )?)
}
pub(super) fn at(index: u64) -> wf_types::version::LogIdentity {
    wf_types::version::LogIdentity::new(1, 7, AppliedPosition::new(index))
}
pub(super) fn driver_limits() -> wf_storage::transaction::RecordBatchTransactionLimits {
    wf_storage::transaction::RecordBatchTransactionLimits {
        codec: wf_ports::log::CodecLimits::new(
            wf_domain::workflow::admission::AdmissionLimits::default(),
        ),
        schema: PolicySchemaLimits::default(),
        maximum_read_steps: 10000,
        maximum_read_bytes: 64 * MIB,
        maximum_staging_bytes: 64 * MIB,
        maximum_adapter_bytes: 32 * MIB,
        workspace_bytes: 512 * MIB,
        outcome_ttl_ms: 1000,
    }
}

fn witness() -> TestResult<WorkflowValuePolicyWitness> {
    let schema = PolicySchemaLimits::default();
    let reference = prepare_original_complete_policy_reference(
        OriginalPolicyReadValue::Absent,
        b"r",
        &canonical_original_etf_safety(),
        schema,
        &mut PolicySchemaCharge::new(schema),
    )?;
    Ok(WorkflowValuePolicyWitness {
        workflow_type: b"r".to_vec(),
        profile: 0,
        generation: reference.generation(),
        digest: reference.digest(),
    })
}
