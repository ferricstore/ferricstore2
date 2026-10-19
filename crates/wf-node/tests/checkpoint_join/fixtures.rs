//! Real separately owned databases; no backend bypass or checkpoint seeding.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use wf_domain::claim::{
    PartitionFilter,
    apply::{ClaimQuery, ClaimStates},
};
use wf_domain::policy::complete::{
    PolicySchemaCharge, PolicySchemaLimits, decode_complete_policy_value_v2,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlState, EntryIdentity};
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedStatus};
use wf_domain::workflow::{CommittedCommand, Create, Operation};
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use wf_ports::log::{CodecLimits, PreparedBytes, RaftLogLimits, RaftLogStore, encode_record_batch};
use wf_ports::state::{DueCursorStep, GroupApplyStore, GroupApplyTransaction, GroupDueCursor};
use wf_storage::log::RedbLogStore;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::{
    RecordBatchTransactionLimits, RecordBatchTransactionOutcome, apply_record_batch_transaction,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity, TimestampMillis,
};

use super::TestResult;

pub(super) const MIB: usize = 1_048_576;
const MIN_FRAME: &str = "4657435000010100000001740000000167000000017a0000000171000525a2a227783574ca47f08fb9036bb53611b0cb7b509f7a81ab36a18cf02c31ec000000000000000100000000000000000001000000010000000170000000016900000000000000000101000000017200000001710000";
const RICH: &str = "46575056000200000000000000070000007446575053000200000001720101ffffffffffffffff010000000757b12c00010100000101000000000000000a000000000000001e0001000000066661696c656401000000000001000000017101010100000001000001000000000000000100000000010000000570686173650100000000000000";
static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) fn directory(label: &str) -> TestResult<PathBuf> {
    let path = std::env::temp_dir().join(format!(
        "wf-join-{}-{}-{label}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir(&path)?;
    Ok(path)
}

pub(super) fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    ))
}

pub(super) fn clock(n: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(n, 0)?, n)?)
}

pub(super) fn id(term: u64, node: u64, index: u64) -> LogIdentity {
    LogIdentity::new(term, node, AppliedPosition::new(index))
}

pub(super) fn hex(s: &str) -> TestResult<Vec<u8>> {
    s.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| Ok(u8::from_str_radix(std::str::from_utf8(p)?, 16)?))
        .collect()
}

pub(super) fn frame(record: &[u8], request: &[u8]) -> TestResult<PreparedBytes> {
    let frame = encode_record_batch(
        PreparedRequest {
            scope: scope()?,
            request: RequestId::new(request.to_vec())?,
            intent: ClientIntent::new(1, [0; 32])?,
            stamp: clock(1)?,
            operation: PreparedOperation::Batch(vec![PreparedBatchItem {
                partition: PartitionId::new(b"p".to_vec())?,
                command: CommittedCommand {
                    id: RecordId::new(record.to_vec())?,
                    now: TimestampMillis::new(1),
                    operation: Operation::Create(Create {
                        workflow_type: b"r".to_vec(),
                        state: b"q".to_vec(),
                        run_at: None,
                        payload: None,
                    }),
                },
                create_priority: 0,
            }]),
        },
        CodecLimits::new(AdmissionLimits::default()),
        64 * MIB,
    )?;
    if record == b"i" && request == b"q" {
        assert_eq!(frame.as_bytes(), hex(MIN_FRAME)?);
        assert_eq!(frame.as_bytes().len(), 115);
        assert_eq!(
            frame.digest().as_bytes().as_slice(),
            hex("d49c43b59fcd1be1f457b8f8fbc0d63f45d719f43c3fe80ed3b3cc18ac2c9840")?
        );
    }
    Ok(frame)
}

pub(super) fn create_pair(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore)> {
    let scope = scope()?;
    let limits = PolicySchemaLimits::default();
    let policy =
        decode_complete_policy_value_v2(&hex(RICH)?, limits, &mut PolicySchemaCharge::new(limits))?;
    let placements = [(PartitionId::new(b"p".to_vec())?, scope.group().clone())];
    let policies = [(scope.tenant().clone(), policy)];
    Ok((
        RedbLogStore::create(
            &dir.join("log.redb"),
            scope.group().clone(),
            scope.recovery().clone(),
            RaftLogLimits::default(),
        )?,
        RedbStateStore::create(
            &dir.join("state.redb"),
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreGenesis {
                initial_time: clock(0)?,
                placements: &placements,
                policies: &policies,
            },
            StateStoreLimits::default(),
        )?,
    ))
}

pub(super) fn open_pair(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore)> {
    let s = scope()?;
    Ok((
        RedbLogStore::open(
            &dir.join("log.redb"),
            s.group().clone(),
            s.recovery().clone(),
            RaftLogLimits::default(),
        )?,
        RedbStateStore::open(
            &dir.join("state.redb"),
            s.group().clone(),
            s.recovery().clone(),
            StateStoreLimits::default(),
        )?,
    ))
}

pub(super) fn apply(
    state: &mut RedbStateStore,
    frame: &PreparedBytes,
    id: LogIdentity,
) -> TestResult<ControlState> {
    let result = apply_record_batch_transaction(
        state,
        &scope()?,
        frame.as_bytes(),
        id,
        RecordBatchTransactionLimits {
            codec: CodecLimits::new(AdmissionLimits::default()),
            schema: PolicySchemaLimits::default(),
            maximum_read_steps: 10_000,
            maximum_read_bytes: 64 * MIB,
            maximum_staging_bytes: 64 * MIB,
            maximum_adapter_bytes: 32 * MIB,
            workspace_bytes: 256 * MIB,
            outcome_ttl_ms: 1_000,
        },
    )?;
    let RecordBatchTransactionOutcome::Retained { control, outcome } = result else {
        return Err("expected durable retained outcome".into());
    };
    assert_eq!(
        control.last_entry(),
        Some(EntryIdentity::new(id, frame.digest()))
    );
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.original_position(), id);
    assert_eq!(outcome.response(), [0]);
    Ok(control)
}

pub(super) fn control(state: &mut RedbStateStore) -> TestResult<ControlState> {
    let s = scope()?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        MIB,
        MIB,
    )?;
    Ok(tx.control()?)
}

pub(super) fn assert_authority(
    state: &mut RedbStateStore,
    expected: ControlState,
    applied: bool,
) -> TestResult {
    let s = scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        MIB,
        MIB,
    )?;
    assert_eq!(tx.control()?, expected);
    assert_eq!(tx.placement(&partition)?.as_ref(), Some(s.group()));
    let schema = PolicySchemaLimits::default();
    let policy = tx
        .installed_policy(
            s.tenant(),
            b"r",
            schema,
            &mut PolicySchemaCharge::new(schema),
        )?
        .ok_or("missing policy")?;
    assert_eq!(policy.encoded(), hex(RICH)?);
    let record = tx.record(s.tenant(), &partition, &RecordId::new(b"i".to_vec())?)?;
    let outcome = tx.outcome(s.tenant(), &RequestId::new(b"q".to_vec())?)?;
    assert_eq!(record.is_some(), applied);
    assert_eq!(outcome.is_some(), applied);
    if let Some(record) = record {
        assert_eq!(record.record.state(), b"q");
        assert_eq!(record.record.version(), 1);
        assert_eq!(record.record.payload(), None);
    }
    if let Some(outcome) = outcome {
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
        assert_eq!(outcome.original_position(), id(1, 7, 0));
        assert_eq!(outcome.response(), [0]);
    }
    assert!(
        tx.record(s.tenant(), &partition, &RecordId::new(b"j".to_vec())?)?
            .is_none()
    );
    assert!(
        tx.outcome(s.tenant(), &RequestId::new(b"j".to_vec())?)?
            .is_none()
    );
    let query = ClaimQuery {
        workflow_type: b"r",
        partitions: PartitionFilter::Any,
        states: ClaimStates::Any,
        exclude_states: &[],
        priority: None,
        worker: b"w",
        lease_ms: 10,
        limit: 10,
    };
    let mut cursor = tx.due_cursor(s.tenant(), query, None, 10, MIB)?;
    let mut rows = 0;
    loop {
        match cursor.next()? {
            DueCursorStep::Row(row) => {
                assert_eq!(row.id, b"i");
                rows += 1;
            }
            DueCursorStep::RangeEnd => break,
            DueCursorStep::BudgetEnd => return Err("unexpected cursor cap".into()),
        }
    }
    assert_eq!(rows, usize::from(applied));
    Ok(())
}

pub(super) fn assert_log(
    log: &mut RedbLogStore,
    expected: wf_ports::log::RaftLogState,
    bodies: &[&PreparedBytes],
) -> TestResult {
    assert_eq!(log.state()?, expected);
    let rows = log.read_page(std::ops::Bound::Unbounded, std::ops::Bound::Unbounded)?;
    assert_eq!(rows.len(), bodies.len());
    for (row, body) in rows.iter().zip(bodies) {
        assert_eq!(row.body(), body.as_bytes());
    }
    Ok(())
}
