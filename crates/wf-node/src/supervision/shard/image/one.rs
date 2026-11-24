//! One record alone: a logged after-image or a cold file's record.

use wf_domain::workflow::admission::{AdmissionLimits, MAX_INLINE_BYTES};

use super::super::command::Reader;
use super::{Kept, Parts, Restored, decode_record, encode_record};
use wf_application::commands::queue::Partition;

/// Encodes one record alone: a logged after-image carries the kept value
/// versions no earlier one logged ([`Kept::Unlogged`]), a cold file all.
pub(crate) fn encode_one(
    partition: &Partition,
    id: &[u8],
    priority: u8,
    record: Parts<'_>,
    kept: Kept,
) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    encode_record(&mut out, partition, id, priority, record, kept)?;
    Some(out)
}

/// Decodes a record encoded by [`encode_one`].
pub(crate) fn decode_one(bytes: &[u8]) -> Option<Restored> {
    let limits = AdmissionLimits::new(MAX_INLINE_BYTES, usize::MAX, 1).ok()?;
    let mut reader = Reader(bytes);
    let restored = decode_record(&mut reader, limits).ok()?;
    reader.0.is_empty().then_some(restored)
}
