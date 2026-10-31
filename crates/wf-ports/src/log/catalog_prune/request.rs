//! Rejecting private decoder predecessor; no synthetic success.
use super::*;
pub(super) fn intent(command: CatalogPruneRequest<'_>) -> Result<ClientIntent, E> {
    let _ = command;
    Err(E::UnsupportedVersion)
}
pub(super) fn encode(
    scope: &RequestScope,
    id: &RequestId,
    stamp: CommittedTime,
    command: CatalogPruneRequest<'_>,
    limits: CodecLimits,
    owned: usize,
) -> Result<PreparedBytes, E> {
    let _ = (scope, id, stamp, command, limits, owned);
    Err(E::UnsupportedVersion)
}
pub(super) fn prepare(
    encoded: &[u8],
    limits: CodecLimits,
    owned: usize,
) -> Result<PreparedCatalogPruneRequest<'_>, E> {
    let _ = (encoded, limits, owned);
    Err(E::UnsupportedVersion)
}
