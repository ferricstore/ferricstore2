//! Closed native outbox meta framing, without engine state.
use super::{E, ReleaseOutboxState};
pub(super) fn encode(state: ReleaseOutboxState, maximum: usize) -> Result<Vec<u8>, E> {
    let _ = (state, maximum);
    Err(E::UnsupportedTag)
}
pub(super) fn inspect(encoded: &[u8], maximum: usize) -> Result<ReleaseOutboxState, E> {
    let _ = (encoded, maximum);
    Err(E::UnsupportedTag)
}
