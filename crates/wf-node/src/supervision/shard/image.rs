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
mod holdings;
mod meta;
mod one;

pub(crate) use holdings::{Holding, HoldingEntry, encode as encode_holdings};
pub(crate) use meta::Kept;
pub(crate) use one::{decode_one, encode_one};

use wf_application::commands::flow::history::HistoryEvent;
use wf_application::commands::flow::record::FlowMeta;
use wf_application::commands::queue::Partition;
use wf_domain::history::EventKind;
use wf_domain::lease::{Lease, LeaseToken};
use wf_domain::workflow::WorkflowRecord;
use wf_domain::workflow::admission::{AdmissionLimits, MAX_INLINE_BYTES};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_storage::cold::Location;
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
    kept: Kept,
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
            meta::encode(out, flow_meta, kept)?;
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

/// Decodes a whole image; `None` when it is not exactly a count and that
/// many records the domain accepts.
pub(crate) fn decode(bytes: &[u8]) -> Option<Image> {
    let limits = AdmissionLimits::new(MAX_INLINE_BYTES, usize::MAX, 1).ok()?;
    let mut reader = Reader(bytes);
    let count = reader.u64().ok()?;
    let mut records = Vec::new();
    for _ in 0..count {
        records.push(decode_record(&mut reader, limits).ok()?);
    }
    let shared_count = reader.u64().ok()?;
    let mut shared = Vec::new();
    for _ in 0..shared_count {
        shared.push(SharedValue {
            partition: Partition::parse(reader.bin().ok()?),
            reference: reader.bin().ok()?.to_vec(),
            bytes: reader.bin().ok()?.to_vec(),
            expires_at: reader.optional_u64().ok()?,
        });
    }
    let policy_count = reader.u64().ok()?;
    let mut policies = Vec::new();
    for _ in 0..policy_count {
        policies.push((
            reader.bin().ok()?.to_vec(),
            reader.u64().ok()?,
            reader.bin().ok()?.to_vec(),
        ));
    }
    let governance_count = reader.u64().ok()?;
    let mut governance = Vec::new();
    for _ in 0..governance_count {
        governance.push((
            reader.u8().ok()?,
            reader.bin().ok()?.to_vec(),
            reader.bin().ok()?.to_vec(),
        ));
    }
    let stub_count = reader.u64().ok()?;
    let mut stubs = Vec::new();
    for _ in 0..stub_count {
        stubs.push(decode_stub(&mut reader).ok()?);
    }
    let holdings = holdings::decode(&mut reader).ok()?;
    reader.0.is_empty().then_some(Image {
        records,
        shared,
        policies,
        governance,
        stubs,
        holdings,
    })
}

/// A decoded shard image: its records, its shared values, the Flow type
/// policies it held (type, generation and encoded policy), then its
/// governance records (kind tag, name and stored form) (ADR-014).
pub(crate) struct Image {
    pub(crate) records: Vec<Restored>,
    pub(crate) shared: Vec<SharedValue>,
    pub(crate) policies: Vec<(Vec<u8>, u64, Vec<u8>)>,
    pub(crate) governance: Vec<(u8, Vec<u8>, Vec<u8>)>,
    /// Finished Flows on disk (ADR-016).
    pub(crate) stubs: Vec<StubImage>,
    /// References Flows hold to values they do not own (audit gap G10).
    pub(crate) holdings: Vec<Holding>,
}

/// A finished Flow on disk as a shard image keeps it (ADR-016): where its
/// image is, and what its catalog and removal entries need.
pub(crate) struct StubImage {
    pub(crate) partition: Partition,
    pub(crate) id: Vec<u8>,
    pub(crate) location: Location,
    pub(crate) flow_type: Vec<u8>,
    pub(crate) state: Vec<u8>,
    pub(crate) updated_at: u64,
    pub(crate) priority: u8,
    pub(crate) removal: u64,
    pub(crate) history: u64,
    pub(crate) values: u64,
}

/// A stub to image: key, location, type, state, update time, priority,
/// removal time, history and value counts.
pub(crate) type StubEntry<'a> = (
    (&'a Partition, &'a [u8]),
    Location,
    (&'a [u8], &'a [u8]),
    (u64, u8, u64),
    (u64, u64),
);

/// Writes the stub section, after the governance section.
pub(crate) fn encode_stubs(out: &mut Vec<u8>, stubs: &[StubEntry<'_>]) -> Option<()> {
    out.extend_from_slice(&u64::try_from(stubs.len()).ok()?.to_be_bytes());
    for ((partition, id), location, (flow_type, state), (updated, priority, removal), counts) in
        stubs
    {
        bin(out, &partition.name())?;
        bin(out, id)?;
        for number in [location.file, location.offset, u64::from(location.length)] {
            out.extend_from_slice(&number.to_be_bytes());
        }
        bin(out, flow_type)?;
        bin(out, state)?;
        out.extend_from_slice(&updated.to_be_bytes());
        out.push(*priority);
        for number in [*removal, counts.0, counts.1] {
            out.extend_from_slice(&number.to_be_bytes());
        }
    }
    Some(())
}

fn decode_stub(reader: &mut Reader<'_>) -> Result<StubImage, Malformed> {
    let partition = Partition::parse(reader.bin()?);
    let id = reader.bin()?.to_vec();
    let location = Location {
        file: reader.u64()?,
        offset: reader.u64()?,
        length: u32::try_from(reader.u64()?).map_err(|_| Malformed)?,
    };
    Ok(StubImage {
        partition,
        id,
        location,
        flow_type: reader.bin()?.to_vec(),
        state: reader.bin()?.to_vec(),
        updated_at: reader.u64()?,
        priority: reader.u8()?,
        removal: reader.u64()?,
        history: reader.u64()?,
        values: reader.u64()?,
    })
}

/// A shared value in a shard image.
pub(crate) struct SharedValue {
    pub(crate) partition: Partition,
    pub(crate) reference: Vec<u8>,
    pub(crate) bytes: Vec<u8>,
    pub(crate) expires_at: Option<u64>,
}

/// A shared value to image: partition, reference, bytes, deadline.
pub(crate) type SharedEntry<'a> = (&'a Partition, &'a [u8], &'a [u8], Option<u64>);

/// Appends the shared-value section after the records.
pub(crate) fn encode_shared(out: &mut Vec<u8>, values: &[SharedEntry<'_>]) -> Option<()> {
    out.extend_from_slice(&u64::try_from(values.len()).ok()?.to_be_bytes());
    for (partition, reference, bytes, expires_at) in values {
        bin(out, &partition.name())?;
        bin(out, reference)?;
        bin(out, bytes)?;
        optional_u64(out, *expires_at);
    }
    Some(())
}

/// Writes the governance section: kind tag, name and stored form each.
pub(crate) fn encode_governance<'a>(
    out: &mut Vec<u8>,
    records: impl ExactSizeIterator<Item = (u8, &'a [u8], &'a [u8])>,
) -> Option<()> {
    out.extend_from_slice(&u64::try_from(records.len()).ok()?.to_be_bytes());
    for (kind, name, value) in records {
        out.push(kind);
        bin(out, name)?;
        bin(out, value)?;
    }
    Some(())
}

/// Writes the policy section: type, generation and encoded policy each.
pub(crate) fn encode_policies(out: &mut Vec<u8>, policies: &[(&[u8], u64, Vec<u8>)]) -> Option<()> {
    out.extend_from_slice(&u64::try_from(policies.len()).ok()?.to_be_bytes());
    for (flow_type, generation, policy) in policies {
        bin(out, flow_type)?;
        out.extend_from_slice(&generation.to_be_bytes());
        bin(out, policy)?;
    }
    Some(())
}
