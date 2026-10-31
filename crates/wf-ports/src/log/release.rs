//! Canonical release values and source cursors; none grants durable authority.
use super::CodecError as E;
use wf_domain::governance::release::{ReleaseIntentRef, ReleaseOutboxState};
mod binding;
mod cursor;
mod intent;
mod meta;
mod mutation;
pub use binding::{ReleaseBindingRef, encode_release_binding, inspect_release_binding};
pub use cursor::ReleaseReconcileCursor;
pub use mutation::reject_workflow_release_backlog;
/// Encodes a completely validated native intent, with exact wire admission.
/// # Errors
/// Refuses malformed fields, wire bounds or allocation failure.
pub fn encode_release_intent(intent: ReleaseIntentRef<'_>, maximum: usize) -> Result<Vec<u8>, E> {
    intent::encode(intent, maximum)
}
/// Completely inspects native framing and returns original borrowed fields.
/// # Errors
/// Refuses malformed/other formats, complete EOF or byte/field limits.
pub fn inspect_release_intent(encoded: &[u8], maximum: usize) -> Result<ReleaseIntentRef<'_>, E> {
    intent::inspect(encoded, maximum)
}
/// Encodes the complete finite native outbox meta.
/// # Errors
/// Refuses malformed scalar meta, byte bounds or allocation failure.
pub fn encode_release_meta(state: ReleaseOutboxState, maximum: usize) -> Result<Vec<u8>, E> {
    meta::encode(state, maximum)
}
/// Completely verifies the native meta frame without another byte owner.
/// # Errors
/// Refuses malformed/other formats, EOF, finite scalars or byte limits.
pub fn inspect_release_meta(encoded: &[u8], maximum: usize) -> Result<ReleaseOutboxState, E> {
    meta::inspect(encoded, maximum)
}
/// Decodes the original source v1 cursor, preserving its accepted integer grammar.
/// # Errors
/// Refuses invalid cursor/numeric/cohort data, prospective owner bounds or allocation.
pub fn decode_release_cursor(
    encoded: Option<&[u8]>,
    shards: u64,
    maximum_owned: usize,
) -> Result<ReleaseReconcileCursor, E> {
    cursor::decode(encoded, shards, maximum_owned)
}
/// Encodes one bounded cursor in canonical ascending source order.
/// # Errors
/// Refuses a wire ceiling or fallible output reservation failure.
pub fn encode_release_cursor(
    cursor: &ReleaseReconcileCursor,
    maximum: usize,
) -> Result<Vec<u8>, E> {
    cursor::encode(cursor, maximum)
}
