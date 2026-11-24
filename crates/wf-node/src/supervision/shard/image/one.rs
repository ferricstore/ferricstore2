//! One record alone: a logged after-image or a cold file's record.

use wf_domain::workflow::admission::{AdmissionLimits, MAX_INLINE_BYTES};

use super::super::command::Reader;
use super::{Kept, Parts, Restored, decode_record, encode_record};
use wf_application::commands::queue::Partition;

/// Encodes one record alone: a logged after-image carries the kept value
/// versions no earlier one logged ([`Kept::Unlogged`]), a cold file all
/// ([`Kept::All`]).
pub(crate) fn encode_one(
    partition: &Partition,
    id: &[u8],
    priority: u8,
    record: Parts<'_>,
    kept: Kept<'_>,
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

/// Where the payload field of a record encoded by [`encode_one`] starts:
/// after its partition, id, priority, type, state, five integers, next run
/// time, run state and lease, as `decode_record` reads them.
fn payload_at(image: &[u8]) -> Option<usize> {
    let mut reader = Reader(image);
    reader.bin().ok()?;
    reader.bin().ok()?;
    reader.u8().ok()?;
    reader.bin().ok()?;
    reader.bin().ok()?;
    for _ in 0..5 {
        reader.u64().ok()?;
    }
    reader.optional_u64().ok()?;
    if reader.u8().ok()? == 1 {
        reader.bin().ok()?;
    }
    match reader.u8().ok()? {
        0 => {}
        1 => {
            reader.bin().ok()?;
            reader.bin().ok()?;
            reader.u64().ok()?;
        }
        _ => return None,
    }
    image.len().checked_sub(reader.0.len())
}

/// An after-image around its payload (perf note P1): the bytes before the
/// payload field, the payload, and the bytes after it; `None` without one.
pub(crate) fn payload_split(image: &[u8]) -> Option<(&[u8], &[u8], &[u8])> {
    let at = payload_at(image)?;
    let (head, field) = image.split_at_checked(at)?;
    let mut reader = Reader(field);
    if reader.u8().ok()? != 1 {
        return None;
    }
    let payload = reader.bin().ok()?;
    Some((head, payload, reader.0))
}

/// An after-image logged without its payload (its field written as
/// [`SHARED`]) with `payload` put back.
pub(crate) fn with_payload(elided: &[u8], payload: &[u8]) -> Option<Vec<u8>> {
    let at = payload_at(elided)?;
    let (head, field) = elided.split_at_checked(at)?;
    let (&flag, tail) = field.split_first()?;
    if flag != SHARED {
        return None;
    }
    let mut out = Vec::with_capacity(elided.len().saturating_add(payload.len()).saturating_add(4));
    out.extend_from_slice(head);
    out.push(1);
    out.extend_from_slice(&u32::try_from(payload.len()).ok()?.to_be_bytes());
    out.extend_from_slice(payload);
    out.extend_from_slice(tail);
    Some(out)
}

/// The payload field's mark for a payload logged once beside the images.
pub(crate) const SHARED: u8 = 2;
