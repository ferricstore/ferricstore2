//! Private same-live database fixtures and explicit fault mutations.

#[cfg(test)]
pub(super) mod fixture_data {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use redb::{Durability, ReadableDatabase, ReadableTable};
    use sha2::{Digest, Sha256};
    use wf_domain::policy::complete::PolicySchemaLimits;
    use wf_domain::workflow::active::LifecycleDefaults;
    use wf_domain::workflow::admission::AdmissionLimits;
    use wf_domain::workflow::control::PreparedDigest;
    use wf_domain::workflow::request::{RequestScope, RetainedStatus};
    use wf_domain::workflow::{Cancel, CommittedCommand, Create, Operation};
    use wf_ports::blob::{
        OriginalValuePartition, OriginalValueRefKind, WorkflowValueReadLimits,
        render_original_value_ref,
    };
    use wf_ports::consensus::{
        PreparedBatchItem, PreparedOperation, PreparedWorkflowValueBatch, WorkflowValueDefaults,
        WorkflowValueItemInput, WorkflowValuePolicyWitness, WorkflowValueTargetGuard,
    };
    use wf_ports::log::{CodecLimits, encode_workflow_value_batch};
    use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
    use wf_types::version::{
        AppliedPosition, CommittedTime, Fence, HlcTimestamp, LogIdentity, TimestampMillis,
    };

    use super::super::{SizedReadFixture, SizedReadTestClock, TestResult};
    use crate::state::redb::{META, ROWS, RedbStateStore, StateStoreGenesis, StateStoreLimits};
    use crate::transaction::{
        RecordBatchTransactionLimits, RecordBatchTransactionOutcome,
        WorkflowValueTransactionLimits, apply_workflow_value_batch_transaction,
    };

    pub(in super::super) fn corrupt_alias(original: &[u8], fault: usize) -> TestResult<Vec<u8>> {
        assert_eq!(original.len(), 121);
        assert!(original.starts_with(b"FWVA\0\x01"));
        assert_eq!(&original[6..16], &[0, 0, 0, 1, b'p', 0, 0, 0, 1, b'i']);
        let mut changed = original.to_vec();
        match fault {
            1 => changed[49..57].copy_from_slice(&10_u64.to_be_bytes()),
            2 => changed[15] = b'j',
            3 => changed[120] ^= 1,
            _ => return Err("unexpected alias fault".into()),
        }
        if fault != 3 {
            let digest = Sha256::digest(&changed[..89]);
            changed[89..].copy_from_slice(&digest);
        }
        Ok(changed)
    }

    pub(in super::super) fn fixture(
        tag: &str,
        fault: usize,
        terminal: bool,
    ) -> TestResult<SizedReadFixture> {
        let path = directory(tag, fault)?;
        let scope = RequestScope::new(
            TenantId::new(b"t".to_vec())?,
            GroupId::new(b"g".to_vec())?,
            RecoveryId::new(b"z".to_vec())?,
        );
        let placements = [(PartitionId::new(b"p".to_vec())?, scope.group().clone())];
        let database = redb::Builder::new().create(path.join("state"))?;
        assert_eq!(database.begin_read()?.list_tables()?.count(), 0);
        let configured = defaults();
        let created = RedbStateStore::create_value_database(
            database,
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreGenesis {
                initial_time: stamp(0)?,
                placements: &placements,
                policies: &[],
            },
            configured,
            StateStoreLimits::default(),
        );
        assert!(created.is_ok(), "{:?}", created.as_ref().err());
        let mut store = created?;
        let prepared = prepared(&scope, configured)?;
        let limits = transaction_limits();
        let encoded = encode_workflow_value_batch(&prepared, limits.record_batch.codec)?;
        let position = LogIdentity::new(1, 7, AppliedPosition::new(0));
        let applied = apply_workflow_value_batch_transaction(
            &mut store,
            &scope,
            encoded.as_bytes(),
            position,
            limits,
        );
        assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
        let RecordBatchTransactionOutcome::Retained { outcome, .. } = applied? else {
            return Err("expected retained real publication".into());
        };
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
        assert_eq!(outcome.original_position(), position);
        let kind = if terminal {
            OriginalValueRefKind::Error
        } else {
            OriginalValueRefKind::Payload
        };
        let version = if terminal { 2 } else { 1 };
        let reference = render_original_value_ref(
            b"i",
            kind,
            version,
            OriginalValuePartition::Explicit(b"p"),
            65_535,
        )
        .map_err(|error| format!("unit reference: {error:?}"))?;
        let old = store.read_workflow_value_batch(
            &mut clock(249)?,
            &scope,
            &[&reference],
            read_limits(),
        )?;
        assert_eq!(old.slots.as_slice(), [Ok(Some(0))]);
        assert_eq!(old.values.as_slice(), [b"123456789".to_vec()]);
        Ok(SizedReadFixture {
            path,
            store,
            scope,
            reference,
        })
    }

    fn prepared(
        scope: &RequestScope,
        defaults: WorkflowValueDefaults,
    ) -> TestResult<PreparedWorkflowValueBatch> {
        let create = Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: Some(b"123456789".to_vec()),
        });
        let cancel = Operation::Cancel(Cancel {
            fence: Fence::new(0),
            lease_token: None,
            expected_state: Some(b"q".to_vec()),
            expected_version: Some(1),
            error: Some(b"123456789".to_vec()),
        });
        let mut items = Vec::new();
        for operation in [create, cancel] {
            items.push(PreparedBatchItem {
                partition: PartitionId::new(b"p".to_vec())?,
                command: CommittedCommand {
                    id: RecordId::new(b"i".to_vec())?,
                    now: TimestampMillis::new(200),
                    operation,
                },
                create_priority: 0,
            });
        }
        let mut inputs = Vec::new();
        for source_index in 0..2 {
            inputs.push(WorkflowValueItemInput {
                source_index,
                policy_index: 0,
                target: WorkflowValueTargetGuard::Unspecified,
                birth: None,
                terminal_ttl_ms: (source_index == 1).then_some(50),
                requested_at: None,
            });
        }
        let witness = WorkflowValuePolicyWitness {
            workflow_type: b"r".to_vec(),
            profile: 0,
            generation: 0,
            digest: PreparedDigest::new([
                0x6e, 0x99, 0x78, 0x63, 0x6a, 0x60, 0x17, 0xb6, 0xe2, 0xab, 0xf8, 0x16, 0x47, 0xa5,
                0xaa, 0xb1, 0xb7, 0x1f, 0x00, 0x3b, 0x16, 0xe2, 0x3a, 0xd9, 0x31, 0xa1, 0xbe, 0xda,
                0x2d, 0x62, 0xb9, 0xa9,
            ]),
        };
        Ok(PreparedWorkflowValueBatch {
            scope: scope.clone(),
            request: RequestId::new(b"sized-unit".to_vec())?,
            stamp: stamp(200)?,
            profile: 6,
            policy_references_captured: Some(true),
            defaults,
            policies: vec![witness],
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

    pub(in super::super) fn read_limits() -> WorkflowValueReadLimits {
        WorkflowValueReadLimits {
            maximum_references: 16,
            maximum_stored_bytes: 64 * 1_048_576,
            maximum_work: 64 * 1_048_576,
            maximum_temporary_bytes: 64 * 1_048_576,
        }
    }

    fn stamp(now: u64) -> TestResult<CommittedTime> {
        Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?)
    }

    pub(in super::super) fn clock(now: u64) -> TestResult<SizedReadTestClock> {
        Ok(SizedReadTestClock {
            hlc: HlcTimestamp::new(now, 0)?,
            wall: TimestampMillis::new(now),
            calls: 0,
            failure: None,
        })
    }

    pub(in super::super) fn table_rows(
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

    pub(in super::super) fn write_row(
        store: &RedbStateStore,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> TestResult {
        let mut write = store.database.begin_write()?;
        write.set_durability(Durability::Immediate)?;
        let mut table = write.open_table(ROWS)?;
        match value {
            Some(value) => {
                assert!(table.insert(key, value)?.is_some());
            }
            None => {
                assert!(table.remove(key)?.is_some());
            }
        }
        drop(table);
        write.commit()?;
        Ok(())
    }

    fn directory(tag: &str, fault: usize) -> TestResult<PathBuf> {
        for suffix in 0..1_000 {
            let path = std::env::temp_dir().join(format!(
                "wf-sized-read-{}-{tag}-{fault}-{suffix}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err("no unique unit directory".into())
    }

    pub(in super::super) fn cleanup(fixture: SizedReadFixture) -> TestResult {
        drop(fixture.store);
        std::fs::remove_dir_all(fixture.path)?;
        Ok(())
    }
}
