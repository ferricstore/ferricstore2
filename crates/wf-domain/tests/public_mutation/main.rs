//! Source scalar callers and authoritative retry inputs; no runtime authority.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::claim::{Claim, ClaimRecord, DueOrder, LaneKey, ScopedRecordId};
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::policy::RetryOverride;
use wf_domain::retry::{JitterSample, PolicyRetry, source_retry_jitter};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::{ApplyError, BatchItem, BatchRejection};
use wf_domain::workflow::caller::{
    CoreMutation, MutationCallerError, MutationFieldValue, MutationInput, NormalizedMutation,
    measure_core_mutation_input, normalize_core_mutation,
};
use wf_domain::workflow::indexed::{
    EntryOrder, IndependentIndexedDecision, IndexRejection, IndexedBatchItem,
    TouchedIndexedGroupView, decide_public_mutation_touched_indexed_batch,
};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_domain::workflow::{
    CommittedCommand, Create, DomainError, Operation, WorkflowRecord, reduce,
};
use wf_types::identity::{GroupId, PartitionId, RecordId};
use wf_types::version::{AppliedPosition, Fence, LogIdentity, TimestampMillis};

mod caller;
mod jitter;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const RAW: usize = 16 * 1_024 * 1_024;

fn normalize<'a>(
    operation: &'a str,
    id: &'a [u8],
    fields: &'a [(&'a str, MutationFieldValue<'a>)],
) -> Result<NormalizedMutation<'a>, MutationCallerError> {
    normalize_core_mutation(
        MutationInput {
            operation,
            id,
            fields,
        },
        &[],
        None,
        AdmissionLimits::default(),
        RAW,
    )
}

fn claimed() -> TestResult<WorkflowRecord> {
    let mut command = CommittedCommand {
        id: RecordId::new(b"i".to_vec())?,
        now: TimestampMillis::new(1),
        operation: Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: None,
        }),
    };
    let record = reduce(None, &command)?
        .into_parts()
        .1
        .ok_or("create absent")?
        .record;
    command.operation = Operation::Claim(Claim {
        expected_state: b"q".to_vec(),
        worker: b"w".to_vec(),
        token: LeaseToken::new(b"l".to_vec())?,
        lease_ms: 30_000,
    });
    Ok(reduce(Some(&record), &command)?
        .into_parts()
        .1
        .ok_or("claim absent")?
        .record)
}

fn with_attempts(record: &WorkflowRecord, attempts: u64) -> TestResult<WorkflowRecord> {
    let (latest_kind, latest_version, latest_fence) = record.latest_event_witness();
    Ok(restore_record(
        WorkflowRecordData {
            id: record.id().clone(),
            workflow_type: record.workflow_type().to_vec(),
            state: record.state().to_vec(),
            version: record.version(),
            fence: record.fence(),
            attempts,
            created_at: record.created_at(),
            updated_at: record.updated_at(),
            next_run_at: record.next_run_at(),
            run_state: record.run_state().map(<[u8]>::to_vec),
            lease: record.lease().cloned(),
            payload: record.payload().map(<[u8]>::to_vec),
            result: record.result().map(<[u8]>::to_vec),
            error: record.error().map(<[u8]>::to_vec),
            latest_kind,
            latest_version,
            latest_fence,
        },
        AdmissionLimits::default(),
    )?)
}

fn decide<'current>(
    record: &'current WorkflowRecord,
    command: &CommittedCommand,
    count: usize,
) -> TestResult<Result<IndependentIndexedDecision<'current>, ApplyError>> {
    let partition = PartitionId::new(b"p".to_vec())?;
    let group = GroupId::new(b"g".to_vec())?;
    let row = ClaimRecord {
        record,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: 99,
    };
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let current = BTreeMap::from([(ScopedRecordId::for_record(&row), Some(row))]);
    let parallel = DueOrder::for_record(&row)
        .map(|due| (due, true))
        .into_iter()
        .collect();
    let members = BTreeMap::new();
    let heads = LaneKey::for_record(&row)
        .map(|lane| (lane, None))
        .into_iter()
        .collect();
    let policies = BTreeMap::from([(b"r".as_slice(), None)]);
    let retries = BTreeMap::from([(b"r".as_slice(), None)]);
    let item = IndexedBatchItem::new(BatchItem::new(&partition, command, b"k"));
    let items = vec![item; count];
    Ok(decide_public_mutation_touched_indexed_batch(
        TouchedIndexedGroupView {
            group: &group,
            placements: &placements,
            current: &current,
            parallel: &parallel,
            fifo_members: &members,
            fifo_heads: &heads,
            policies: &policies,
            retry_policies: &retries,
        },
        &items,
        EntryOrder {
            identity: LogIdentity::new(1, 7, AppliedPosition::new(1)),
            first_ordinal: 0,
        },
        b"public",
        AdmissionLimits::default(),
    ))
}
