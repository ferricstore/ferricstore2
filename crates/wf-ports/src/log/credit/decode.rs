//! Fallible materialization only after the complete actual frame was inspected.

use super::{encode, model::PreparedCreditRequest, scan};
use crate::log::{CodecError as E, CodecLimits, keys, reader::copy, writer::Writer};
use sha2::{Digest, Sha256};
use wf_domain::workflow::{control::PreparedDigest, request::RequestScope};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};

pub(super) fn prepare(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedCreditRequest<'_>, E> {
    let raw = scan::inspect(encoded, limits, maximum_owned)?;
    let isolation = keys::scope(raw.header.scope())?;
    let scope = RequestScope::new(
        TenantId::new(copy(raw.header.tenant)?).map_err(|_| E::Malformed)?,
        GroupId::new(copy(raw.header.group)?).map_err(|_| E::Malformed)?,
        RecoveryId::new(copy(raw.header.recovery)?).map_err(|_| E::Malformed)?,
    );
    let mut values = Vec::new();
    values
        .try_reserve_exact(raw.ids.len())
        .map_err(|_| E::Allocation)?;
    for id in &raw.ids {
        values.push(copy(id)?);
    }
    let mut mutation = raw.mutation;
    mutation.install_ids(values);
    let prepared = PreparedCreditRequest {
        encoded,
        scope,
        request: RequestId::new(copy(raw.header.request)?).map_err(|_| E::Malformed)?,
        stamp: raw.header.stamp,
        intent: raw.header.intent,
        digest: PreparedDigest::new(Sha256::digest(encoded).into()),
        isolation,
        credit_scope: copy(raw.scope)?,
        shard_count: raw.shard_count,
        configuration: raw.configuration,
        mutation,
        owned_bytes: raw.owned_bytes,
    };
    let mut check = Writer::verify(&encoded[raw.body_at..], limits.admission().decision_bytes())?;
    encode::body(&mut check, prepared.command())?;
    check.finish_verification()?;
    Ok(prepared)
}
