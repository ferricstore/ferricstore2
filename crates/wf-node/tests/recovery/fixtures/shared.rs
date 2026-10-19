//! Real producers/drivers and independent complete physical observations.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use redb::{ReadableDatabase, ReadableTable, TableDefinition};
use wf_consensus::apply::{apply_control_entry, read_applied_state};
use wf_domain::{
    policy::complete::{PolicySchemaCharge, PolicySchemaLimits, decode_complete_policy_value_v2},
    workflow::{
        Cancel, CommittedCommand, Create, Operation,
        admission::AdmissionLimits,
        caller::{MutationFieldValue as V, MutationInput, normalize_core_mutation},
        control::{ControlState, EntryStamp},
        request::{ClientIntent, RequestScope},
    },
};
use wf_ports::{
    consensus::{
        AppliedRaftMembership, PreparedBatchItem, PreparedOperation, PreparedRequest,
        RaftMembership, RaftMembershipLimits,
    },
    log::{
        CodecLimits, PreparedBytes, RaftLogEntry, RaftLogLimits, RaftLogStore,
        encode_independent_record_batch, encode_public_mutation, encode_raft_blank,
        encode_raft_membership, encode_record_batch,
    },
    state::GroupApplyStore,
};
use wf_storage::{
    log::RedbLogStore,
    state::{RedbStateStore, StateStoreGenesis, StateStoreLimits},
    transaction::{
        RecordBatchTransactionLimits, RecordBatchTransactionOutcome,
        apply_independent_record_batch_transaction, apply_public_mutation_transaction,
        apply_record_batch_transaction,
    },
};
use wf_types::{
    identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId},
    version::{AppliedPosition, CommittedTime, Fence, HlcTimestamp, LogIdentity, TimestampMillis},
};

use super::TestResult;

pub(crate) type Rows = BTreeMap<Vec<u8>, Vec<u8>>;
pub(crate) const MIB: usize = 1_048_576;
const RICH: &str = "46575056000200000000000000070000007446575053000200000001720101ffffffffffffffff010000000757b12c00010100000101000000000000000a000000000000001e0001000000066661696c656401000000000001000000017101010100000001000001000000000000000100000000010000000570686173650100000000000000";
static NEXT: AtomicU64 = AtomicU64::new(0);

pub(crate) fn directory(label: &str) -> TestResult<PathBuf> {
    let path = std::env::temp_dir().join(format!(
        "wf-recovery-{}-{}-{label}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir(&path)?;
    Ok(path)
}

pub(crate) fn scope() -> TestResult<RequestScope> {
    scope_for(b"t", b"g", b"z")
}

pub(crate) fn scope_for(tenant: &[u8], group: &[u8], recovery: &[u8]) -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(tenant.to_vec())?,
        GroupId::new(group.to_vec())?,
        RecoveryId::new(recovery.to_vec())?,
    ))
}

pub(crate) fn clock(physical: u64, logical: u16, wall: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(
        HlcTimestamp::new(physical, logical)?,
        wall,
    )?)
}

pub(crate) fn at(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}

pub(crate) fn hex(s: &str) -> TestResult<Vec<u8>> {
    assert_eq!(s.len() % 2, 0);
    s.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

pub(crate) fn driver_limits() -> RecordBatchTransactionLimits {
    RecordBatchTransactionLimits {
        codec: CodecLimits::new(AdmissionLimits::default()),
        schema: PolicySchemaLimits::default(),
        maximum_read_steps: 10_000,
        maximum_read_bytes: 64 * MIB,
        maximum_staging_bytes: 64 * MIB,
        maximum_adapter_bytes: 32 * MIB,
        workspace_bytes: 256 * MIB,
        outcome_ttl_ms: 1_000,
    }
}

pub(crate) fn create_pair(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore)> {
    create_pair_for(dir, &scope()?)
}

pub(crate) fn create_pair_for(
    dir: &Path,
    s: &RequestScope,
) -> TestResult<(RedbLogStore, RedbStateStore)> {
    let schema = PolicySchemaLimits::default();
    let policy =
        decode_complete_policy_value_v2(&hex(RICH)?, schema, &mut PolicySchemaCharge::new(schema))?;
    let placements = [(PartitionId::new(b"p".to_vec())?, s.group().clone())];
    let policies = [(s.tenant().clone(), policy)];
    Ok((
        RedbLogStore::create(
            &dir.join("log.redb"),
            s.group().clone(),
            s.recovery().clone(),
            RaftLogLimits::default(),
        )?,
        RedbStateStore::create(
            &dir.join("state.redb"),
            s.group().clone(),
            s.recovery().clone(),
            StateStoreGenesis {
                initial_time: clock(0, 0, 0)?,
                placements: &placements,
                policies: &policies,
            },
            StateStoreLimits::default(),
        )?,
    ))
}

pub(crate) fn open_pair(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore)> {
    open_pair_for(dir, &scope()?)
}

pub(crate) fn open_pair_for(
    dir: &Path,
    s: &RequestScope,
) -> TestResult<(RedbLogStore, RedbStateStore)> {
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

pub(crate) fn blank(stamp: EntryStamp) -> TestResult<PreparedBytes> {
    let s = scope()?;
    Ok(encode_raft_blank(
        s.group(),
        s.recovery(),
        stamp,
        RaftMembershipLimits::default(),
        64 * MIB,
    )?)
}

pub(crate) fn member(stamp: EntryStamp, extra_node: u64) -> TestResult<PreparedBytes> {
    let s = scope()?;
    let membership = RaftMembership::new(
        vec![vec![1, 2], vec![2, 3], vec![1, 2]],
        vec![
            (1, "a".into()),
            (2, String::new()),
            (3, "x\0".into()),
            (extra_node, "β".into()),
        ],
        RaftMembershipLimits::default(),
    )?;
    Ok(encode_raft_membership(
        s.group(),
        s.recovery(),
        stamp,
        &membership,
        RaftMembershipLimits::default(),
        64 * MIB,
    )?)
}

pub(crate) fn create_op() -> Operation {
    Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(vec![0, 255]),
    })
}

pub(crate) fn cancel_op() -> Operation {
    Operation::Cancel(Cancel {
        fence: Fence::new(0),
        lease_token: None,
        expected_state: Some(b"q".to_vec()),
        expected_version: None,
        error: Some(b"cancelled".to_vec()),
    })
}

pub(crate) fn frame(
    profile: u16,
    request: &[u8],
    now: u64,
    entries: Vec<(&[u8], Operation)>,
) -> TestResult<PreparedBytes> {
    frame_for(&scope()?, profile, request, now, entries)
}

pub(crate) fn frame_for(
    s: &RequestScope,
    profile: u16,
    request: &[u8],
    now: u64,
    entries: Vec<(&[u8], Operation)>,
) -> TestResult<PreparedBytes> {
    if profile == 1 {
        assert_eq!(entries.len(), 1);
        let (id, operation) = &entries[0];
        let Operation::Cancel(value) = operation else {
            return Err("expected public Cancel fixture".into());
        };
        assert_eq!(value.fence.get(), 0);
        assert_eq!(value.expected_state.as_deref(), Some(b"q".as_slice()));
        assert_eq!(value.expected_version, None);
        assert_eq!(value.error.as_deref(), Some(b"cancelled".as_slice()));
        let fields = [
            ("fencing_token", V::Integer(0)),
            ("expect_state", V::Bytes(b"q")),
            ("reason", V::Bytes(b"cancelled")),
            ("now_ms", V::Integer(i128::from(now))),
        ];
        let caller = normalize_core_mutation(
            MutationInput {
                operation: "cancel",
                id,
                fields: &fields,
            },
            &[],
            Some(b"p"),
            AdmissionLimits::default(),
            16 * MIB,
        )?;
        return Ok(encode_public_mutation(
            s,
            &RequestId::new(request.to_vec())?,
            &caller,
            clock(now, 0, now)?,
            driver_limits().codec,
            driver_limits().workspace_bytes,
        )?
        .0);
    }
    let items = entries
        .into_iter()
        .map(|(id, operation)| {
            Ok(PreparedBatchItem {
                partition: PartitionId::new(b"p".to_vec())?,
                create_priority: 0,
                command: CommittedCommand {
                    id: RecordId::new(id.to_vec())?,
                    now: TimestampMillis::new(now),
                    operation,
                },
            })
        })
        .collect::<TestResult<Vec<_>>>()?;
    let operation = PreparedOperation::Batch(items);
    let request = PreparedRequest {
        scope: s.clone(),
        request: RequestId::new(request.to_vec())?,
        intent: ClientIntent::new(1, [0; 32])?,
        stamp: clock(now, 0, now)?,
        operation,
    };
    let limits = driver_limits();
    Ok(match profile {
        5 => encode_record_batch(request, limits.codec, limits.workspace_bytes)?,
        6 => encode_independent_record_batch(request, limits.codec, limits.workspace_bytes)?,
        _ => return Err("unsupported fixture profile".into()),
    })
}

pub(crate) fn append(log: &mut RedbLogStore, index: u64, body: &[u8]) -> TestResult {
    log.append(vec![RaftLogEntry::new(at(index), body.to_vec())])?;
    Ok(())
}

pub(crate) fn apply_control(
    state: &mut RedbStateStore,
    index: u64,
    body: &[u8],
) -> TestResult<ControlState> {
    let s = scope()?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        10_000,
        64 * MIB,
        32 * MIB,
    )?;
    Ok(apply_control_entry(
        tx,
        s.group(),
        s.recovery(),
        body,
        at(index),
        RaftMembershipLimits::default(),
        256 * MIB,
    )?)
}

pub(crate) fn apply_normal(
    state: &mut RedbStateStore,
    index: u64,
    body: &[u8],
    profile: u16,
) -> TestResult<RecordBatchTransactionOutcome> {
    let s = scope()?;
    Ok(match profile {
        5 => apply_record_batch_transaction(state, &s, body, at(index), driver_limits())?,
        6 => {
            apply_independent_record_batch_transaction(state, &s, body, at(index), driver_limits())?
        }
        1 => apply_public_mutation_transaction(state, &s, body, at(index), driver_limits())?,
        _ => return Err("unsupported fixture profile".into()),
    })
}

pub(crate) fn applied(
    state: &mut RedbStateStore,
) -> TestResult<(ControlState, AppliedRaftMembership)> {
    let s = scope()?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        10_000,
        64 * MIB,
        32 * MIB,
    )?;
    Ok(read_applied_state(
        &tx,
        RaftMembershipLimits::default(),
        256 * MIB,
    )?)
}

pub(crate) fn bytes_table(path: &Path, name: &str) -> TestResult<Rows> {
    let database = redb::Builder::new().open(path)?;
    let read = database.begin_read()?;
    let table = read.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
    table
        .iter()?
        .map(|row| {
            let (key, value) = row?;
            Ok((key.value().to_vec(), value.value().to_vec()))
        })
        .collect()
}

pub(crate) fn state_rows(dir: &Path) -> TestResult<Rows> {
    bytes_table(&dir.join("state.redb"), "wf_state_rows_v1")
}

pub(crate) fn log_meta(dir: &Path) -> TestResult<Rows> {
    bytes_table(&dir.join("log.redb"), "wf_raft_meta_v1")
}

pub(crate) fn log_rows(dir: &Path) -> TestResult<BTreeMap<u64, Vec<u8>>> {
    let database = redb::Builder::new().open(dir.join("log.redb"))?;
    let read = database.begin_read()?;
    let table = read.open_table(TableDefinition::<u64, &[u8]>::new("wf_raft_log_v1"))?;
    table
        .iter()?
        .map(|row| {
            let (key, value) = row?;
            Ok((key.value(), value.value().to_vec()))
        })
        .collect()
}
