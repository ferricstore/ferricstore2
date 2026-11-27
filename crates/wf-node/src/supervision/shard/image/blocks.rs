//! A shard image as blocks, for a group snapshot (ADR-018 phase 3): its
//! head (shared values, Flow type policies and governance records), each
//! record on its own, an empty block, then its trailer (stubs, holdings and
//! kept versions on disk). A snapshot carries each shard this way, so
//! neither its capture nor its install holds a shard's whole image; the
//! whole-image layout of a node's own snapshots is the same sections
//! around a count of records.

use wf_application::commands::queue::Partition;

use super::super::command::Reader;
use super::{
    Holding, Restored, SharedValue, Stored, StubImage, decode_record, decode_stub, holdings,
    limits, stored,
};

/// The sections before the records.
pub(crate) struct Head {
    pub(crate) shared: Vec<SharedValue>,
    pub(crate) policies: Vec<(Vec<u8>, u64, Vec<u8>)>,
    pub(crate) governance: Vec<(u8, Vec<u8>, Vec<u8>)>,
}

/// The sections after the records.
pub(crate) struct Trailer {
    pub(crate) stubs: Vec<StubImage>,
    pub(crate) holdings: Vec<Holding>,
    pub(crate) stored: Vec<Stored>,
}

/// Reads the shared values, policies and governance records.
pub(super) fn read_head(reader: &mut Reader<'_>) -> Option<Head> {
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
    Some(Head {
        shared,
        policies,
        governance,
    })
}

/// Reads the stubs, holdings and kept versions on disk.
pub(super) fn read_trailer(reader: &mut Reader<'_>) -> Option<Trailer> {
    let stub_count = reader.u64().ok()?;
    let mut stubs = Vec::new();
    for _ in 0..stub_count {
        stubs.push(decode_stub(reader).ok()?);
    }
    let holdings = holdings::decode(reader).ok()?;
    let stored = stored::decode(reader).ok()?;
    Some(Trailer {
        stubs,
        holdings,
        stored,
    })
}

/// A head block; `None` unless it is exactly one.
pub(crate) fn head(block: &[u8]) -> Option<Head> {
    let mut reader = Reader(block);
    let head = read_head(&mut reader)?;
    reader.0.is_empty().then_some(head)
}

/// A record block; `None` unless it is exactly one record the domain
/// accepts.
pub(crate) fn record(block: &[u8]) -> Option<Restored> {
    let mut reader = Reader(block);
    let record = decode_record(&mut reader, limits()?).ok()?;
    reader.0.is_empty().then_some(record)
}

/// A trailer block; `None` unless it is exactly one.
pub(crate) fn trailer(block: &[u8]) -> Option<Trailer> {
    let mut reader = Reader(block);
    let trailer = read_trailer(&mut reader)?;
    reader.0.is_empty().then_some(trailer)
}
