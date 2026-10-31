//! Separate verifier for the frozen application-owned public Claim V1 mapping.
use super::{ClientIntent, CodecError, NormalizedClaimDue};
use crate::log::public_mutation::intent::{header, map, scalar};
use sha2::{Digest, Sha256};
mod compound;
mod fields;
type IntentSink<'a> = dyn FnMut(&[u8]) -> Result<(), CodecError> + 'a;

pub(in crate::log) fn prepare(
    tenant: &[u8],
    caller: &NormalizedClaimDue<'_>,
    maximum: usize,
) -> Result<ClientIntent, CodecError> {
    let names = caller
        .states()
        .1
        .len()
        .checked_add(caller.partitions().1.len())
        .and_then(|n| n.checked_add(caller.values().1.len()))
        .ok_or(CodecError::LengthOverflow)?;
    if names > maximum / 9 {
        return Err(CodecError::ByteLimit);
    }
    let mut measured = 0_usize;
    canonical(tenant, caller, &mut |bytes| {
        measured = measured
            .checked_add(bytes.len())
            .ok_or(CodecError::LengthOverflow)?;
        if measured > maximum {
            return Err(CodecError::ByteLimit);
        }
        Ok(())
    })?;
    let mut hash = Sha256::new();
    canonical(tenant, caller, &mut |bytes| {
        hash.update(bytes);
        Ok(())
    })?;
    ClientIntent::new(1, hash.finalize().into()).map_err(|_| CodecError::UnsupportedVersion)
}

fn canonical(
    tenant: &[u8],
    caller: &NormalizedClaimDue<'_>,
    out: &mut IntentSink<'_>,
) -> Result<(), CodecError> {
    out(b"WF-INTENT\0V1\0")?;
    let optional = [
        caller.priority().is_some(),
        caller.payload_max_bytes().is_some(),
        caller.governance_limit_scope().is_some(),
        caller.governance_shard_id().is_some(),
        caller.requested_now_ms().is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    map(5, out, |out| {
        scalar(5, b"args", out)?;
        map(11 + optional, out, |out| fields::write(caller, out))?;
        for (key, value) in [
            ("normalization_version", "1"),
            ("operation", "workflow.claim_due"),
            ("schema_version", "1"),
        ] {
            scalar(5, key.as_bytes(), out)?;
            scalar(5, value.as_bytes(), out)?;
        }
        scalar(5, b"tenant_id", out)?;
        scalar(6, tenant, out)
    })
}
