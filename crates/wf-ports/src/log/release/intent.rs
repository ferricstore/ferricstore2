//! Complete native intent inspection and exact-reservation encoding.
use super::{E, ReleaseIntentRef};
use crate::log::{CodecLimits, reader::Reader, writer::Writer};
use wf_domain::governance::release::{LimitEnforcement, validate_release_intent};
use wf_types::{bounds::MAX_RECORD_ID_BYTES as MAX_KEY, version::TimestampMillis};
pub(super) fn encode(intent: ReleaseIntentRef<'_>, maximum: usize) -> Result<Vec<u8>, E> {
    validate_release_intent(intent).map_err(|_| E::Malformed)?;
    let mut measure = Writer::measure(maximum);
    write(intent, &mut measure)?;
    let mut output = Writer::allocate(measure.len(), maximum)?;
    write(intent, &mut output)?;
    output.finish()
}
pub(super) fn inspect(encoded: &[u8], maximum: usize) -> Result<ReleaseIntentRef<'_>, E> {
    if encoded.len() > maximum {
        return Err(E::ByteLimit);
    }
    let mut input = Reader::new(encoded, CodecLimits::default());
    if input.take(4)? != b"FWRI" {
        return Err(E::UnsupportedTag);
    }
    if input.u16()? != 1 {
        return Err(E::UnsupportedVersion);
    }
    let flow_id = input.nonempty(MAX_KEY)?;
    let partition = input.optional(|input| input.raw(MAX_KEY))?;
    let scope = input.nonempty(MAX_KEY)?;
    let shard_id = input.u64()?;
    let reservation_id = input.nonempty(256)?;
    let enforcement = match input.byte()? {
        1 => LimitEnforcement::StrictGlobal,
        2 => LimitEnforcement::ApproximateGlobal,
        _ => return Err(E::UnsupportedTag),
    };
    let created_at = TimestampMillis::new(input.u64()?);
    input.finish()?;
    let intent = ReleaseIntentRef {
        flow_id,
        partition,
        scope,
        shard_id,
        reservation_id,
        enforcement,
        created_at,
    };
    validate_release_intent(intent).map_err(|_| E::Malformed)?;
    Ok(intent)
}
pub(super) fn write(intent: ReleaseIntentRef<'_>, output: &mut Writer<'_>) -> Result<(), E> {
    output.write(b"FWRI\0\x01")?;
    output.raw(intent.flow_id, MAX_KEY)?;
    output.optional(intent.partition, |output, p| output.raw(p, MAX_KEY))?;
    output.raw(intent.scope, MAX_KEY)?;
    output.u64(intent.shard_id)?;
    output.raw(intent.reservation_id, 256)?;
    output.byte(match intent.enforcement {
        LimitEnforcement::StrictGlobal => 1,
        LimitEnforcement::ApproximateGlobal => 2,
    })?;
    output.u64(intent.created_at.get())
}
