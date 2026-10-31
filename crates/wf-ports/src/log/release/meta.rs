//! Closed native outbox meta framing, without engine state.
use super::{E, ReleaseOutboxState};
use crate::log::{CodecLimits, reader::Reader, writer::Writer};
use wf_domain::governance::release::validate_release_state;
pub(super) fn encode(state: ReleaseOutboxState, maximum: usize) -> Result<Vec<u8>, E> {
    validate_release_state(state).map_err(|_| E::Malformed)?;
    let mut output = Writer::allocate(22, maximum)?;
    output.write(b"FWRO\0\x01")?;
    output.u64(state.head)?;
    output.u64(state.tail)?;
    output.finish()
}
pub(super) fn inspect(encoded: &[u8], maximum: usize) -> Result<ReleaseOutboxState, E> {
    if encoded.len() > maximum {
        return Err(E::ByteLimit);
    }
    let mut input = Reader::new(encoded, CodecLimits::default());
    if input.take(4)? != b"FWRO" {
        return Err(E::UnsupportedTag);
    }
    if input.u16()? != 1 {
        return Err(E::UnsupportedVersion);
    }
    let state = ReleaseOutboxState {
        head: input.u64()?,
        tail: input.u64()?,
    };
    input.finish()?;
    validate_release_state(state).map_err(|_| E::Malformed)?;
    Ok(state)
}
