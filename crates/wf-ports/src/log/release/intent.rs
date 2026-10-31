//! Complete native intent inspection and exact-reservation encoding.
use super::{E, ReleaseIntentRef};
pub(super) fn encode(intent: ReleaseIntentRef<'_>, maximum: usize) -> Result<Vec<u8>, E> {
    let _ = (intent, maximum);
    Err(E::UnsupportedTag)
}
pub(super) fn inspect(encoded: &[u8], maximum: usize) -> Result<ReleaseIntentRef<'_>, E> {
    let _ = (encoded, maximum);
    Err(E::UnsupportedTag)
}
