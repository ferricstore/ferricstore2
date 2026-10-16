//! Recording contract spies; real atomicity/recovery is tested by the native driver.

#![forbid(unsafe_code)]

use std::cell::RefCell;

use wf_domain::claim::apply::ClaimQuery;
use wf_domain::claim::{DueOrder, LaneKey, LaneOrder};
use wf_domain::history::HistoryEntry;
use wf_domain::policy::complete::{
    CompletePolicyValue, PolicySchemaCharge, PolicySchemaLimits, decode_complete_policy_value_v2,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlState, plan_control};
use wf_domain::workflow::request::{
    ClientIntent, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
};
use wf_domain::workflow::{CommittedCommand, Create, Operation, RecordWrite, reduce};
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use wf_ports::log::{
    CodecLimits, RecordBatchApplyError, encode_record_batch, prepare_record_batch,
};
use wf_ports::state::{
    DueCursorStep, GroupApplyChanges, GroupApplyStore, GroupApplyTransaction, GroupDueCursor,
    StateTransactionError, StoredWorkflowRecord,
};
use wf_storage::transaction::{
    RecordBatchTransactionError, RecordBatchTransactionLimits, RecordBatchTransactionOutcome,
    apply_record_batch_transaction,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity, TimestampMillis,
};

mod admission;
mod gate;
mod independent;
mod load;
mod public_mutation;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const MIB: usize = 1_024 * 1_024;
const MIN_FRAME: &str = "4657435000010100000001740000000167000000017a0000000171000525a2a227783574ca47f08fb9036bb53611b0cb7b509f7a81ab36a18cf02c31ec000000000000000100000000000000000001000000010000000170000000016900000000000000000101000000017200000001710000";
const MIN_INTENT: &str = "25a2a227783574ca47f08fb9036bb53611b0cb7b509f7a81ab36a18cf02c31ec";
const RICH: &str = "46575056000200000000000000070000007446575053000200000001720101ffffffffffffffff010000000757b12c00010100000101000000000000000a000000000000001e0001000000066661696c656401000000000001000000017101010100000001000001000000000000000100000000010000000570686173650100000000000000";

struct ObservedStore {
    scope: RequestScope,
    control: ControlState,
    retained: RefCell<Option<RetainedOutcome>>,
    retained_present: bool,
    records: Vec<StoredWorkflowRecord>,
    policy_bytes: Vec<(Vec<u8>, Vec<u8>)>,
    placement: Option<GroupId>,
    parallel: bool,
    calls: RefCell<Vec<&'static str>>,
    queried_types: RefCell<Vec<Vec<u8>>>,
    failure: Option<(&'static str, StateTransactionError)>,
    committed: bool,
    next_control: Option<ControlState>,
    written_outcome: Option<RetainedOutcome>,
    writes: Vec<RecordWrite>,
    histories: Vec<HistoryEntry>,
    captured_admission: Option<AdmissionLimits>,
}

struct ObservedTransaction<'store> {
    store: &'store mut ObservedStore,
    group: &'store GroupId,
    recovery: &'store RecoveryId,
}

struct ObservedCursor;

fn hex(value: &str) -> TestResult<Vec<u8>> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn clock(now: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?)
}

fn log(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}

fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    ))
}

fn limits() -> RecordBatchTransactionLimits {
    RecordBatchTransactionLimits {
        codec: CodecLimits::new(AdmissionLimits::default()),
        schema: PolicySchemaLimits::default(),
        maximum_read_steps: 1_000,
        maximum_read_bytes: 64 * MIB,
        maximum_staging_bytes: 64 * MIB,
        maximum_adapter_bytes: 32 * MIB,
        workspace_bytes: 256 * MIB,
        outcome_ttl_ms: 1_000,
    }
}

fn observed_store() -> TestResult<ObservedStore> {
    Ok(ObservedStore {
        scope: scope()?,
        control: ControlState::initial(clock(0)?),
        retained: RefCell::new(None),
        retained_present: false,
        records: Vec::new(),
        policy_bytes: Vec::new(),
        placement: Some(GroupId::new(b"g".to_vec())?),
        parallel: true,
        calls: RefCell::new(Vec::new()),
        queried_types: RefCell::new(Vec::new()),
        failure: None,
        committed: false,
        next_control: None,
        written_outcome: None,
        writes: Vec::new(),
        histories: Vec::new(),
        captured_admission: None,
    })
}

fn create(workflow_type: &[u8]) -> Operation {
    Operation::Create(Create {
        workflow_type: workflow_type.to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: None,
    })
}

fn frame(now: u64, entries: Vec<(&[u8], Operation)>) -> TestResult<Vec<u8>> {
    let items = entries
        .into_iter()
        .map(|(id, operation)| {
            Ok(PreparedBatchItem {
                partition: PartitionId::new(b"p".to_vec())?,
                command: CommittedCommand {
                    id: RecordId::new(id.to_vec())?,
                    now: TimestampMillis::new(now),
                    operation,
                },
                create_priority: 0,
            })
        })
        .collect::<TestResult<Vec<_>>>()?;
    let prepared = encode_record_batch(
        PreparedRequest {
            scope: scope()?,
            request: RequestId::new(b"q".to_vec())?,
            intent: ClientIntent::new(1, [0; 32])?,
            stamp: clock(now)?,
            operation: PreparedOperation::Batch(items),
        },
        limits().codec,
        limits().workspace_bytes,
    )?;
    Ok(prepared.as_bytes().to_vec())
}

fn original() -> TestResult<RetainedOutcome> {
    Ok(RetainedOutcome::new(
        RetainedMetadata {
            scope: scope()?,
            request: RequestId::new(b"q".to_vec())?,
            intent: ClientIntent::record_batch(
                hex(MIN_INTENT)?.try_into().map_err(|_| "digest length")?,
            ),
            original_position: log(0),
            original_time: TimestampMillis::new(1),
            expires_at: TimestampMillis::new(1_001),
            status: RetainedStatus::Accepted,
        },
        1,
        vec![0],
        AdmissionLimits::default(),
    )?)
}

fn processed() -> TestResult<ControlState> {
    let bytes = hex(MIN_FRAME)?;
    let decoded = prepare_record_batch(&bytes, limits().codec, limits().workspace_bytes)?;
    let header = decoded.header(log(0));
    Ok(plan_control(
        ControlState::initial(clock(0)?),
        header.identity,
        wf_domain::workflow::control::EntryStamp::Stamped(header.stamp),
    )?
    .next())
}

impl ObservedStore {
    fn observe(&self, name: &'static str) -> Result<(), StateTransactionError> {
        self.calls.borrow_mut().push(name);
        if let Some((failed, error)) = self.failure
            && failed == name
        {
            return Err(error);
        }
        Ok(())
    }
}

impl GroupApplyStore for ObservedStore {
    type Transaction<'s> = ObservedTransaction<'s>;

    fn begin_apply<'s>(
        &'s mut self,
        group: &'s GroupId,
        recovery: &'s RecoveryId,
        admission: AdmissionLimits,
        maximum_read_steps: usize,
        maximum_read_bytes: usize,
        maximum_temporary_bytes: usize,
    ) -> Result<Self::Transaction<'s>, StateTransactionError> {
        self.observe("begin")?;
        if group != self.scope.group() || recovery != self.scope.recovery() {
            return Err(StateTransactionError::Corrupt);
        }
        if maximum_read_steps == 0 || maximum_read_bytes == 0 || maximum_temporary_bytes == 0 {
            return Err(StateTransactionError::Limit);
        }
        self.captured_admission = Some(admission);
        Ok(ObservedTransaction {
            store: self,
            group,
            recovery,
        })
    }
}

impl GroupDueCursor for ObservedCursor {
    fn next(&mut self) -> Result<DueCursorStep<'_>, StateTransactionError> {
        Err(StateTransactionError::Unavailable)
    }
    fn position(&self) -> Option<DueOrder<'_>> {
        None
    }
}
