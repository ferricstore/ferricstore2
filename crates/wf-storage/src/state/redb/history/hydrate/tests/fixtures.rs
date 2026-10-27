//! Genuine publications and explicit same-live database observations for hydration.

#[cfg(test)]
pub(super) mod fixture_data {
    use std::{collections::BTreeMap, path::PathBuf};

    use redb::{Durability, ReadableDatabase, ReadableTable};
    use sha2::{Digest, Sha256};
    use wf_domain::policy::complete::{
        OriginalPolicyReadValue, PolicySchemaCharge, PolicySchemaLimits,
        canonical_original_etf_safety,
    };
    use wf_domain::workflow::active::LifecycleDefaults;
    use wf_domain::workflow::admission::AdmissionLimits;
    use wf_domain::workflow::control::PreparedDigest;
    use wf_domain::workflow::request::{RequestScope, RetainedStatus};
    use wf_domain::workflow::{CommittedCommand, Create, Operation, Transition};
    use wf_ports::consensus::{
        PreparedBatchItem, PreparedOperation, PreparedWorkflowValueBatch, WorkflowValueDefaults,
        WorkflowValueItemInput, WorkflowValuePolicyWitness, WorkflowValueTargetGuard,
    };
    use wf_ports::log::{
        CodecLimits, encode_workflow_stored_value_batch, prepare_original_complete_policy_reference,
    };
    use wf_ports::state::{
        WorkflowHistoryDirection, WorkflowHistoryQuery, WorkflowHistoryReadLimits,
    };
    use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
    use wf_types::version::{
        AppliedPosition, CommittedTime, Fence, HlcTimestamp, LogIdentity, TimestampMillis,
    };

    use super::super::{TestClock, TestResult};
    use crate::state::redb::{META, ROWS, RedbStateStore, StateStoreGenesis, StateStoreLimits};
    use crate::transaction::{
        RecordBatchTransactionLimits, RecordBatchTransactionOutcome,
        WorkflowValueTransactionLimits, apply_workflow_stored_value_batch_transaction,
    };
    use wf_domain::history::values::{HistoryValueLimits, HistoryValueReturn};
    use wf_domain::value::{OriginalValueContext, OriginalValueLimits};
    use wf_ports::blob::HistoryValueReadConversionLimits;
    use wf_ports::state::{WorkflowHistoryHydrationLimits, WorkflowHistoryHydrationRequest};

    pub(in super::super) fn fixture(
        tag: &str,
    ) -> TestResult<(PathBuf, RedbStateStore, RequestScope)> {
        let path = directory(tag)?;
        let database = redb::Builder::new()
            .set_cache_size(0)
            .create(path.join("state"))?;
        let (store, scope) = publish(database)?;
        Ok((path, store, scope))
    }

    pub(in super::super) fn publish(
        database: redb::Database,
    ) -> TestResult<(RedbStateStore, RequestScope)> {
        let scope = RequestScope::new(
            TenantId::new(b"t".to_vec())?,
            GroupId::new(b"g".to_vec())?,
            RecoveryId::new(b"z".to_vec())?,
        );
        let placements = [(PartitionId::new(b"p".to_vec())?, scope.group().clone())];
        let mut store = RedbStateStore::create_stored_value_database(
            database,
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
        let request = prepared(&scope)?;
        let limits = transaction_limits();
        let encoded = encode_workflow_stored_value_batch(&request, limits.record_batch.codec)?;
        let outcome = apply_workflow_stored_value_batch_transaction(
            &mut store,
            &scope,
            encoded.as_bytes(),
            LogIdentity::new(1, 7, AppliedPosition::new(0)),
            limits,
        )?;
        let RecordBatchTransactionOutcome::Retained { outcome, .. } = outcome else {
            return Err("missing actual consumed publication".into());
        };
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
        Ok((store, scope))
    }

    fn prepared(scope: &RequestScope) -> TestResult<PreparedWorkflowValueBatch> {
        let operations = [
            (
                b"i",
                Operation::Create(Create {
                    workflow_type: b"r".to_vec(),
                    state: b"q".to_vec(),
                    run_at: None,
                    payload: Some(b"first".to_vec()),
                }),
            ),
            (
                b"i",
                Operation::Transition(Transition {
                    from: b"q".to_vec(),
                    to: b"a".to_vec(),
                    fence: Fence::new(0),
                    lease_token: None,
                    run_at: None,
                    payload: Some(b"second".to_vec()),
                }),
            ),
            (
                b"j",
                Operation::Create(Create {
                    workflow_type: b"r".to_vec(),
                    state: b"q".to_vec(),
                    run_at: None,
                    payload: Some(b"other".to_vec()),
                }),
            ),
        ];
        let items = operations
            .into_iter()
            .map(|(id, operation)| {
                Ok(PreparedBatchItem {
                    partition: PartitionId::new(b"p".to_vec())?,
                    command: CommittedCommand {
                        id: RecordId::new(id.to_vec())?,
                        now: TimestampMillis::new(100),
                        operation,
                    },
                    create_priority: 0,
                })
            })
            .collect::<TestResult<Vec<_>>>()?;
        let inputs = (0..3)
            .map(|source_index| WorkflowValueItemInput {
                source_index,
                policy_index: 0,
                target: WorkflowValueTargetGuard::Unspecified,
                birth: None,
                terminal_ttl_ms: None,
                requested_at: None,
            })
            .collect();
        let schema = PolicySchemaLimits::default();
        let reference = prepare_original_complete_policy_reference(
            OriginalPolicyReadValue::Absent,
            b"r",
            &canonical_original_etf_safety(),
            schema,
            &mut PolicySchemaCharge::new(schema),
        )?;
        Ok(PreparedWorkflowValueBatch {
            scope: scope.clone(),
            request: RequestId::new(b"history-unit".to_vec())?,
            stamp: stamp(100)?,
            profile: 6,
            policy_references_captured: Some(true),
            defaults: defaults(),
            policies: vec![WorkflowValuePolicyWitness {
                workflow_type: b"r".to_vec(),
                profile: 0,
                generation: reference.generation(),
                digest: reference.digest(),
            }],
            operation: PreparedOperation::Batch(items),
            inputs,
        })
    }

    fn defaults() -> WorkflowValueDefaults {
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

    fn history_limits() -> WorkflowHistoryReadLimits {
        WorkflowHistoryReadLimits {
            maximum_entries: 100,
            maximum_row_reads: 1_000,
            maximum_inspected_bytes: 64 * 1_048_576,
            maximum_returned_bytes: 64 * 1_048_576,
            maximum_work: 64 * 1_048_576,
            maximum_temporary_bytes: 64 * 1_048_576,
        }
    }

    fn history_query(limit: usize) -> WorkflowHistoryQuery<'static> {
        WorkflowHistoryQuery {
            limit,
            boundary_event: None,
            direction: WorkflowHistoryDirection::Ascending,
            expected_control: None,
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
        key: &[u8],
        value: Option<&[u8]>,
    ) -> TestResult {
        let mut write = store.database.begin_write()?;
        write.set_durability(Durability::Immediate)?;
        let mut table = write.open_table(ROWS)?;
        if let Some(value) = value {
            assert!(table.insert(key, value)?.is_some());
        } else {
            assert!(table.remove(key)?.is_some());
        }
        drop(table);
        write.commit()?;
        Ok(())
    }

    fn directory(tag: &str) -> TestResult<PathBuf> {
        for suffix in 0..1_000 {
            let path = std::env::temp_dir()
                .join(format!("wf-history-{}-{tag}-{suffix}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err("no unique unit directory".into())
    }
    pub(in super::super) fn directory_for_fault(tag: &str) -> TestResult<PathBuf> {
        directory(tag)
    }

    pub(in super::super) fn clock() -> TestResult<TestClock> {
        Ok(TestClock {
            hlc: HlcTimestamp::new(101, 0)?,
            wall: TimestampMillis::new(101),
            calls: 0,
            failure: None,
        })
    }

    pub(in super::super) fn query(max_bytes: i128) -> WorkflowHistoryHydrationRequest<'static> {
        WorkflowHistoryHydrationRequest {
            query: history_query(1),
            value_return: HistoryValueReturn::Enabled { max_bytes },
            decode_context: OriginalValueContext {
                safe_atoms: None,
                active_exports: None,
                local_node: None,
                local_functions: None,
            },
        }
    }

    pub(in super::super) fn limits() -> WorkflowHistoryHydrationLimits {
        WorkflowHistoryHydrationLimits {
            history: history_limits(),
            values: wf_ports::blob::WorkflowValueReadLimits {
                maximum_references: 192,
                maximum_stored_bytes: 64 * 1_048_576,
                maximum_work: 64 * 1_048_576,
                maximum_temporary_bytes: 64 * 1_048_576,
            },
            conversion: HistoryValueReadConversionLimits {
                maximum_references: 192,
                maximum_work: 1_000_000,
                maximum_temporary_bytes: 8_000_000,
            },
            projection: HistoryValueLimits {
                maximum_entries: 64,
                maximum_references: 192,
                maximum_read_results: 192,
                maximum_writes: 384,
                maximum_work_units: 20_000_000,
                maximum_requested_bytes: 128_000_000,
                decoder: OriginalValueLimits {
                    max_encoded_bytes: 262_144,
                    max_nodes: 4_096,
                    max_edges: 8_192,
                    max_depth: 256,
                    max_work_units: 200_000,
                },
            },
            maximum_work: 512 * 1_048_576,
            maximum_temporary_bytes: 512 * 1_048_576,
        }
    }
}
