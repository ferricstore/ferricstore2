//! Actual consumed metadata publication and explicit same-live fault writes.

#[cfg(test)]
pub(super) mod fixture_data {

    use redb::{Durability, ReadableDatabase, ReadableTable};
    use sha2::{Digest, Sha256};
    use std::{collections::BTreeMap, path::PathBuf};
    use wf_domain::policy::complete::{
        OriginalPolicyReadValue, PolicySchemaCharge, PolicySchemaLimits,
        canonical_original_etf_safety,
    };
    use wf_domain::workflow::{
        CommittedCommand, Create, Operation,
        active::LifecycleDefaults,
        admission::AdmissionLimits,
        control::PreparedDigest,
        request::{RequestScope, RetainedStatus},
    };
    use wf_ports::consensus::{
        PreparedBatchItem, PreparedOperation, PreparedWorkflowValueBatch, WorkflowValueDefaults,
        WorkflowValueItemInput, WorkflowValuePolicyWitness, WorkflowValueTargetGuard,
    };
    use wf_ports::log::{
        CodecLimits, MetadataBatchInput, MetadataBatchLimits, encode_workflow_metadata_batch,
        encode_workflow_stored_value_batch, prepare_original_complete_policy_reference,
    };
    use wf_ports::state::{
        WorkflowRecordReadLimits, WorkflowRecordReadPort, WorkflowRecordReadQuery,
    };
    use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
    use wf_types::version::{
        AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity, TimestampMillis,
    };

    use super::super::TestResult;
    use crate::state::redb::{META, ROWS, RedbStateStore, StateStoreGenesis, StateStoreLimits};
    use crate::transaction::{
        RecordBatchTransactionLimits, RecordBatchTransactionOutcome,
        WorkflowValueTransactionLimits, apply_workflow_metadata_batch_transaction,
    };

    pub(in super::super) fn setup(
        tag: &str,
    ) -> TestResult<(PathBuf, RedbStateStore, RequestScope)> {
        let directory = directory(tag)?;
        let scope = RequestScope::new(
            TenantId::new(b"t".to_vec())?,
            GroupId::new(b"g".to_vec())?,
            RecoveryId::new(b"z".to_vec())?,
        );
        let placements = [(PartitionId::new(b"p".to_vec())?, scope.group().clone())];
        let mut store = RedbStateStore::create_metadata_value_database(
            redb::Builder::new().create(directory.join("state"))?,
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreGenesis {
                initial_time: stamp(0)?,
                placements: &placements,
                policies: &[],
            },
            defaults(),
            StateStoreLimits::default(),
        )?;
        publish(
            &mut store,
            &scope,
            100,
            Operation::Create(Create {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                run_at: None,
                payload: Some(b"data".to_vec()),
            }),
            7,
        )?;
        Ok((directory, store, scope))
    }

    pub(in super::super) fn publish(
        store: &mut RedbStateStore,
        scope: &RequestScope,
        now: u64,
        operation: Operation,
        metadata: u8,
    ) -> TestResult {
        let target = if matches!(operation, Operation::Create(_)) {
            WorkflowValueTargetGuard::Absent
        } else {
            let old = store.read_workflow_record(
                scope,
                &PartitionId::new(b"p".to_vec())?,
                &RecordId::new(b"i".to_vec())?,
                WorkflowRecordReadQuery::default(),
                limits(),
            )?;
            WorkflowValueTargetGuard::Present {
                incarnation: old
                    .capture
                    .ok_or("actual capture")?
                    .values
                    .lifecycle
                    .incarnation,
            }
        };
        let schema = PolicySchemaLimits::default();
        let reference = prepare_original_complete_policy_reference(
            OriginalPolicyReadValue::Absent,
            b"r",
            &canonical_original_etf_safety(),
            schema,
            &mut PolicySchemaCharge::new(schema),
        )?;
        let prepared = PreparedWorkflowValueBatch {
            scope: scope.clone(),
            request: RequestId::new(now.to_be_bytes().to_vec())?,
            stamp: stamp(now)?,
            profile: 5,
            policy_references_captured: Some(true),
            defaults: defaults(),
            policies: vec![WorkflowValuePolicyWitness {
                workflow_type: b"r".to_vec(),
                profile: 0,
                generation: reference.generation(),
                digest: reference.digest(),
            }],
            operation: PreparedOperation::Batch(vec![PreparedBatchItem {
                partition: PartitionId::new(b"p".to_vec())?,
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
                birth: None,
                terminal_ttl_ms: None,
                requested_at: None,
            }],
        };
        let tx_limits = transaction_limits();
        let inner = encode_workflow_stored_value_batch(&prepared, tx_limits.record_batch.codec)?;
        let mut data = b"FSV2\x02\x83\x68\x03\x6d\0\0\0\x12wf.record.metadata\x61\x01\x74\0\0\0\x01\x6d\0\0\0\x0aattributes\x61".to_vec();
        data.push(metadata);
        let encoded = encode_workflow_metadata_batch(
            MetadataBatchInput {
                stored_batch: inner.as_bytes(),
                metadata: &[Some(&data)],
            },
            MetadataBatchLimits {
                codec: tx_limits.record_batch.codec,
                maximum_owned_bytes: 64 * 1_048_576,
                maximum_work_units: 64 * 1_048_576,
            },
        )?;
        let RecordBatchTransactionOutcome::Retained { outcome, .. } =
            apply_workflow_metadata_batch_transaction(
                store,
                scope,
                encoded.as_bytes(),
                LogIdentity::new(1, 7, AppliedPosition::new(now - 100)),
                tx_limits,
            )?
        else {
            return Err("actual consumed publication".into());
        };
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
        Ok(())
    }

    pub(in super::super) fn defaults() -> WorkflowValueDefaults {
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

    fn transaction_limits() -> WorkflowValueTransactionLimits {
        WorkflowValueTransactionLimits {
            record_batch: RecordBatchTransactionLimits {
                codec: CodecLimits::new(AdmissionLimits::default()),
                schema: PolicySchemaLimits::default(),
                maximum_read_steps: 10_000,
                maximum_read_bytes: 64 * 1_048_576,
                maximum_staging_bytes: 64 * 1_048_576,
                maximum_adapter_bytes: 32 * 1_048_576,
                workspace_bytes: 512 * 1_048_576,
                outcome_ttl_ms: 1_000,
            },
            maximum_value_rows: 1_024,
            maximum_value_bytes: 64 * 1_048_576,
            maximum_value_work: 64 * 1_048_576,
            maximum_value_temporary_bytes: 64 * 1_048_576,
        }
    }

    fn stamp(now: u64) -> TestResult<CommittedTime> {
        Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?)
    }

    pub(in super::super) fn limits() -> WorkflowRecordReadLimits {
        WorkflowRecordReadLimits {
            maximum_row_reads: 16,
            maximum_inspected_bytes: 64 * 1_048_576,
            maximum_returned_bytes: 64 * 1_048_576,
            maximum_work: 64 * 1_048_576,
            maximum_temporary_bytes: 64 * 1_048_576,
        }
    }

    pub(in super::super) fn rows(
        store: &RedbStateStore,
        metadata: bool,
    ) -> TestResult<BTreeMap<Vec<u8>, Vec<u8>>> {
        let read = store.database.begin_read()?;
        let table = read.open_table(if metadata { META } else { ROWS })?;
        table
            .iter()?
            .map(|row| {
                let (key, value) = row?;
                Ok((key.value().to_vec(), value.value().to_vec()))
            })
            .collect()
    }

    pub(in super::super) fn rewrite(
        store: &RedbStateStore,
        table: bool,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> TestResult {
        let mut write = store.database.begin_write()?;
        write.set_durability(Durability::Immediate)?;
        let mut rows = write.open_table(if table { META } else { ROWS })?;
        if let Some(bytes) = value {
            rows.insert(key, bytes)?;
        } else {
            rows.remove(key)?;
        }
        drop(rows);
        write.commit()?;
        Ok(())
    }

    fn directory(tag: &str) -> TestResult<PathBuf> {
        for suffix in 0..1_000 {
            let path = std::env::temp_dir().join(format!(
                "wf-record-query-{tag}-{}-{suffix}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err("no query fixture directory".into())
    }
}
