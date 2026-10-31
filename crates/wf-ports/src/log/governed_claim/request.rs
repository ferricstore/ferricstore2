//! Complete borrowed frame inspection before materializing existing Claim owners.
use super::{
    CapturedGovernedClaimPayment, CodecLimits, DecodedRequest, E, GovernedClaimPayment as P,
    GovernedClaimPaymentRef as R, PreparedBytes, PreparedGovernedClaim, payment,
};
use crate::{
    consensus::{PreparedClaimPhase, PreparedOperation, PreparedRequest},
    log::{
        claim,
        common::RawHeader,
        keys,
        reader::{Reader, copy},
        writer::Writer,
    },
};
use sha2::{Digest, Sha256};
use wf_domain::workflow::{
    control::PreparedDigest,
    request::{ClientIntent, RequestScope},
};
use wf_types::{
    identity::{GroupId, RecoveryId, RequestId, TenantId},
    version::{CommittedTime, HlcTimestamp},
};
const INTENT: &[u8] = b"wf-governed-claim-intent-v1\0";

/// Encodes a private paid Claim, retaining the original spent identifiers.
/// # Errors
/// Refuses complete framing, duplicate IDs, numeric limits or prospective ownership.
pub fn encode_governed_claim_request(
    scope: &RequestScope,
    request: &RequestId,
    stamp: CommittedTime,
    phase: &PreparedClaimPhase,
    paid: P<'_>,
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedBytes, E> {
    keys::scope_length([
        scope.tenant().as_bytes(),
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
    ])?;
    let maximum = limits.admission().decision_bytes();
    let mut measure = Writer::measure(maximum);
    write_header(&mut measure, scope, request, stamp)?;
    claim::write(phase, &mut measure, limits)?;
    payment::write(&mut measure, paid)?;
    let required = measure.len().checked_add(2048).ok_or(E::LengthOverflow)?;
    if required > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut out = Writer::allocate(measure.len(), maximum)?;
    let digest_at = write_header(&mut out, scope, request, stamp)?;
    claim::write(phase, &mut out, limits)?;
    payment::write(&mut out, paid)?;
    let mut bytes = out.finish()?;
    let (_, _, payment, body_at) = scan(&bytes, limits)?;
    payment::validate(payment, maximum_owned - required)?;
    let digest = intent_digest(&bytes, body_at, payment)?;
    bytes
        .get_mut(digest_at..digest_at + 32)
        .ok_or(E::Malformed)?
        .copy_from_slice(&digest);
    Ok(PreparedBytes {
        digest: PreparedDigest::new(Sha256::digest(&bytes).into()),
        bytes,
    })
}
fn write_header(
    out: &mut Writer<'_>,
    scope: &RequestScope,
    request: &RequestId,
    stamp: CommittedTime,
) -> Result<usize, E> {
    out.write(b"FWCP\0\x01\x11")?;
    out.raw(scope.tenant().as_bytes(), 8192)?;
    for field in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        request.as_bytes(),
    ] {
        out.raw(field, 256)?;
    }
    out.u16(19)?;
    let at = out.len();
    out.write(&[0; 32])?;
    out.u64(stamp.hlc().physical().get())?;
    out.u16(stamp.hlc().logical())?;
    out.u64(stamp.wall().get())?;
    Ok(at)
}
fn header<'a>(reader: &mut Reader<'a>) -> Result<RawHeader<'a>, E> {
    if reader.take(4)? != b"FWCP" {
        return Err(E::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(E::UnsupportedVersion);
    }
    if reader.byte()? != 17 {
        return Err(E::UnsupportedTag);
    }
    let tenant = reader.nonempty(8192)?;
    let group = reader.nonempty(256)?;
    let recovery = reader.nonempty(256)?;
    let request = reader.nonempty(256)?;
    if reader.u16()? != 19 {
        return Err(E::UnsupportedVersion);
    }
    let intent = ClientIntent::governed_claim(reader.fixed()?);
    let hlc = HlcTimestamp::new(reader.u64()?, reader.u16()?).map_err(|_| E::Malformed)?;
    let stamp = CommittedTime::new(hlc, reader.u64()?).map_err(|_| E::Malformed)?;
    Ok(RawHeader {
        operation_tag: 17,
        tenant,
        group,
        recovery,
        request,
        intent,
        stamp,
    })
}
fn scan(encoded: &[u8], limits: CodecLimits) -> Result<(RawHeader<'_>, usize, R<'_>, usize), E> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(E::ByteLimit);
    }
    let mut reader = Reader::new(encoded, limits);
    let header = header(&mut reader)?;
    keys::scope_length(header.scope())?;
    let body_at = encoded.len() - reader.remaining();
    let filters = claim::occurrences(&mut reader)?;
    let payment = payment::read(&mut reader, encoded)?;
    reader.finish()?;
    Ok((header, filters, payment, body_at))
}
fn intent_digest(encoded: &[u8], body_at: usize, payment: R<'_>) -> Result<[u8; 32], E> {
    let suffix = payment.ids.len().checked_add(10).ok_or(E::LengthOverflow)?;
    let end = encoded.len().checked_sub(suffix).ok_or(E::Malformed)?;
    let mut hash = Sha256::new();
    hash.update(INTENT);
    hash.update(encoded.get(body_at..end).ok_or(E::Malformed)?);
    Ok(hash.finalize().into())
}
/// Scans the whole frame and admits owners before materializing its Claim query.
/// # Errors
/// Refuses complete syntax/intent mismatch, duplicate IDs or prospective ownership.
pub fn prepare_governed_claim_request(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedGovernedClaim<'_>, E> {
    let (header, filters, payment, body_at) = scan(encoded, limits)?;
    if intent_digest(encoded, body_at, payment)? != *header.intent.digest() {
        return Err(E::Malformed);
    }
    let bound = claim::apply::decoder_bound(encoded.len(), filters)?
        .checked_add(std::mem::size_of::<DecodedRequest<'_>>())
        .ok_or(E::LengthOverflow)?;
    if bound > maximum_owned {
        return Err(E::OwnedLimit);
    }
    payment::validate(payment, maximum_owned - bound)?;
    let isolation = keys::scope(header.scope())?;
    let scope = RequestScope::new(
        TenantId::new(copy(header.tenant)?).map_err(|_| E::Malformed)?,
        GroupId::new(copy(header.group)?).map_err(|_| E::Malformed)?,
        RecoveryId::new(copy(header.recovery)?).map_err(|_| E::Malformed)?,
    );
    let request = RequestId::new(copy(header.request)?).map_err(|_| E::Malformed)?;
    let mut reader = Reader::new(encoded, limits);
    reader.take(body_at)?;
    let (phase, filters) = claim::read(&mut reader)?;
    reader.take(payment.body.len())?;
    reader.finish()?;
    let digest = PreparedDigest::new(Sha256::digest(encoded).into());
    Ok(PreparedGovernedClaim {
        capture: CapturedGovernedClaimPayment {
            payment,
            prepared: digest,
            requested_limit: phase.query.limit,
        },
        decoded: DecodedRequest {
            request: PreparedRequest {
                scope,
                request,
                intent: header.intent,
                stamp: header.stamp,
                operation: PreparedOperation::ClaimPhase(phase),
            },
            encoded,
            digest,
            isolation,
            state_keys: Vec::new(),
            claim_filters: Some(filters),
            value_metadata: None,
            limits,
            record_batch_intent: Err(E::UnsupportedTag),
        },
    })
}
