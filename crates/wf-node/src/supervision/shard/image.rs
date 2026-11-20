//! Snapshot images of shard state (ADR-013).
//!
//! An image is a `u64` record count, then each record in creation order: its
//! partition, id and priority, then every field of the domain record. Decoding
//! rebuilds each record through `restore_record`, so a value the domain would
//! not accept fails the decode instead of entering a shard. Layout follows the
//! log records: binaries are a `u32` length and bytes, options a `0`/`1` byte
//! then the value, integers big-endian. A record ends with `0`, or `1` and
//! its fields beyond the lifecycle record (ADR-014, [`meta`]), then `0`, or
//! `1` and its history events ([`history`]).

mod history;
mod meta;

use wf_application::commands::flow::history::HistoryEvent;
use wf_application::commands::flow::record::FlowMeta;
use wf_application::commands::queue::Partition;
use wf_domain::history::EventKind;
use wf_domain::lease::{Lease, LeaseToken};
use wf_domain::workflow::WorkflowRecord;
use wf_domain::workflow::admission::{AdmissionLimits, MAX_INLINE_BYTES};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_types::identity::RecordId;
use wf_types::version::{Fence, TimestampMillis};

use super::command::{Malformed, Reader, bin, optional_bin, optional_u64};

/// Event kinds by their byte code; a kind missing here cannot be imaged.
pub(super) const KINDS: [EventKind; 14] = [
    EventKind::Created,
    EventKind::Claimed,
    EventKind::Transitioned,
    EventKind::StepContinued,
    EventKind::Completed,
    EventKind::Failed,
    EventKind::Retry,
    EventKind::LeaseExtended,
    EventKind::Cancelled,
    EventKind::ChildCompleted,
    EventKind::ChildFailed,
    EventKind::ChildCancelled,
    EventKind::ChildrenCancelled,
    EventKind::ChildrenSpawned,
];

/// One record of an image, ready to store in a shard.
pub(crate) struct Restored {
    pub(crate) partition: Partition,
    pub(crate) id: Vec<u8>,
    pub(crate) priority: u8,
    pub(crate) record: WorkflowRecord,
    pub(crate) meta: Option<FlowMeta>,
    /// History events, carried by snapshot images only.
    pub(crate) history: Option<Vec<HistoryEvent>>,
}

/// Appends one record; `None` when a field is longer than `u32::MAX` or its
/// event kind has no code.
pub(crate) fn encode_record(
    out: &mut Vec<u8>,
    partition: &Partition,
    id: &[u8],
    priority: u8,
    (record, flow_meta, events): Parts<'_>,
) -> Option<()> {
    bin(out, &partition.name())?;
    bin(out, id)?;
    out.push(priority);
    bin(out, record.workflow_type())?;
    bin(out, record.state())?;
    for number in [
        record.version(),
        record.fence().get(),
        record.attempts(),
        record.created_at().get(),
        record.updated_at().get(),
    ] {
        out.extend_from_slice(&number.to_be_bytes());
    }
    optional_u64(out, record.next_run_at().map(TimestampMillis::get));
    optional_bin(out, record.run_state())?;
    match record.lease() {
        None => out.push(0),
        Some(lease) => {
            out.push(1);
            bin(out, &lease.worker)?;
            bin(out, lease.token.as_bytes())?;
            out.extend_from_slice(&lease.deadline.get().to_be_bytes());
        }
    }
    optional_bin(out, record.payload())?;
    optional_bin(out, record.result())?;
    optional_bin(out, record.error())?;
    let (kind, version, fence) = record.latest_event_witness();
    out.push(u8::try_from(KINDS.iter().position(|known| *known == kind)?).ok()?);
    out.extend_from_slice(&version.to_be_bytes());
    out.extend_from_slice(&fence.get().to_be_bytes());
    match flow_meta {
        None => out.push(0),
        Some(flow_meta) => {
            out.push(1);
            meta::encode(out, flow_meta)?;
        }
    }
    match events {
        None => out.push(0),
        Some(events) => {
            out.push(1);
            history::encode(out, events)?;
        }
    }
    Some(())
}

/// A record, its fields beyond the lifecycle record, and its history when
/// the image carries it.
pub(crate) type Parts<'a> = (
    &'a WorkflowRecord,
    Option<&'a FlowMeta>,
    Option<&'a [HistoryEvent]>,
);

fn decode_record(reader: &mut Reader<'_>, limits: AdmissionLimits) -> Result<Restored, Malformed> {
    let partition = Partition::parse(reader.bin()?);
    let id = reader.bin()?.to_vec();
    let priority = reader.u8()?;
    let workflow_type = reader.bin()?.to_vec();
    let state = reader.bin()?.to_vec();
    let [version, fence, attempts, created_at, updated_at] = [
        reader.u64()?,
        reader.u64()?,
        reader.u64()?,
        reader.u64()?,
        reader.u64()?,
    ];
    let next_run_at = reader.optional_u64()?.map(TimestampMillis::new);
    let run_state = reader.optional_bin()?;
    let lease = match reader.u8()? {
        0 => None,
        1 => Some(Lease {
            worker: reader.bin()?.to_vec(),
            token: LeaseToken::new(reader.bin()?.to_vec()).map_err(|_| Malformed)?,
            deadline: TimestampMillis::new(reader.u64()?),
        }),
        _ => return Err(Malformed),
    };
    let payload = reader.optional_bin()?;
    let result = reader.optional_bin()?;
    let error = reader.optional_bin()?;
    let latest_kind = *KINDS.get(usize::from(reader.u8()?)).ok_or(Malformed)?;
    let latest_version = reader.u64()?;
    let latest_fence = Fence::new(reader.u64()?);
    let data = WorkflowRecordData {
        id: RecordId::new(id.clone()).map_err(|_| Malformed)?,
        workflow_type,
        state,
        version,
        fence: Fence::new(fence),
        attempts,
        created_at: TimestampMillis::new(created_at),
        updated_at: TimestampMillis::new(updated_at),
        next_run_at,
        run_state,
        lease,
        payload,
        result,
        error,
        latest_kind,
        latest_version,
        latest_fence,
    };
    let record = restore_record(data, limits).map_err(|_| Malformed)?;
    let meta = match reader.u8()? {
        0 => None,
        1 => Some(meta::decode(reader)?),
        _ => return Err(Malformed),
    };
    let history = match reader.u8()? {
        0 => None,
        1 => Some(history::decode(reader)?),
        _ => return Err(Malformed),
    };
    Ok(Restored {
        partition,
        id,
        priority,
        record,
        meta,
        history,
    })
}

/// Encodes one record alone, as a logged after-image.
pub(crate) fn encode_one(
    partition: &Partition,
    id: &[u8],
    priority: u8,
    record: Parts<'_>,
) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    encode_record(&mut out, partition, id, priority, record)?;
    Some(out)
}

/// Decodes a record encoded by [`encode_one`].
pub(crate) fn decode_one(bytes: &[u8]) -> Option<Restored> {
    let limits = AdmissionLimits::new(MAX_INLINE_BYTES, usize::MAX, 1).ok()?;
    let mut reader = Reader(bytes);
    let restored = decode_record(&mut reader, limits).ok()?;
    reader.0.is_empty().then_some(restored)
}

/// Decodes a whole image; `None` when it is not exactly a count and that
/// many records the domain accepts.
pub(crate) fn decode(bytes: &[u8]) -> Option<Vec<Restored>> {
    let limits = AdmissionLimits::new(MAX_INLINE_BYTES, usize::MAX, 1).ok()?;
    let mut reader = Reader(bytes);
    let count = reader.u64().ok()?;
    let mut records = Vec::new();
    for _ in 0..count {
        records.push(decode_record(&mut reader, limits).ok()?);
    }
    reader.0.is_empty().then_some(records)
}
