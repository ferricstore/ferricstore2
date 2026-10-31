//! Full borrowed inspection before bounded ownership of original header fields.

use super::model::PreparedCatalogAckRequest;
use crate::log::{
    CodecError as E, CodecLimits, PreparedBytes,
    common::RawHeader,
    keys,
    reader::{Reader, copy},
    writer::Writer,
};
use sha2::{Digest, Sha256};
use wf_domain::{
    governance::catalog::CreditCatalogAckRequest,
    workflow::{
        control::PreparedDigest,
        request::{ClientIntent, RequestScope},
    },
};
use wf_types::{
    identity::{GroupId, RecoveryId, RequestId, TenantId},
    version::{CommittedTime, HlcTimestamp},
};

const INTENT: &[u8] = b"wf-credit-catalog-ack-intent-v1\0";

pub(super) fn encode(
    scope: &RequestScope,
    request: &RequestId,
    stamp: CommittedTime,
    command: CreditCatalogAckRequest,
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedBytes, E> {
    command.validate().map_err(|_| E::Malformed)?;
    keys::scope_length([
        scope.tenant().as_bytes(),
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
    ])?;
    let mut hash = Sha256::new();
    hash.update(INTENT);
    hash.update(command.expected_head.to_be_bytes());
    hash.update(command.up_to.to_be_bytes());
    let intent: [u8; 32] = hash.finalize().into();
    let mut measure = Writer::measure(limits.admission().decision_bytes());
    write_header(&mut measure, scope, request, stamp, &intent)?;
    body(&mut measure, command)?;
    if measure.len().checked_add(2048).ok_or(E::LengthOverflow)? > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut output = Writer::allocate(measure.len(), limits.admission().decision_bytes())?;
    write_header(&mut output, scope, request, stamp, &intent)?;
    body(&mut output, command)?;
    let bytes = output.finish()?;
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
    intent: &[u8; 32],
) -> Result<(), E> {
    out.write(b"FWCP\0\x01\x0b")?;
    out.raw(scope.tenant().as_bytes(), 8192)?;
    for part in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        request.as_bytes(),
    ] {
        out.raw(part, 256)?;
    }
    out.u16(13)?;
    out.write(intent)?;
    out.u64(stamp.hlc().physical().get())?;
    out.u16(stamp.hlc().logical())?;
    out.u64(stamp.wall().get())
}

fn body(out: &mut Writer<'_>, command: CreditCatalogAckRequest) -> Result<(), E> {
    out.u64(command.expected_head)?;
    out.u64(command.up_to)
}

pub(super) fn header<'a>(reader: &mut Reader<'a>) -> Result<RawHeader<'a>, E> {
    if reader.take(4)? != b"FWCP" {
        return Err(E::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(E::UnsupportedVersion);
    }
    if reader.byte()? != 11 {
        return Err(E::UnsupportedTag);
    }
    let tenant = reader.nonempty(8192)?;
    let group = reader.nonempty(256)?;
    let recovery = reader.nonempty(256)?;
    let request = reader.nonempty(256)?;
    if reader.u16()? != 13 {
        return Err(E::UnsupportedVersion);
    }
    let intent = ClientIntent::credit_catalog_ack(reader.fixed()?);
    let hlc = HlcTimestamp::new(reader.u64()?, reader.u16()?).map_err(|_| E::Malformed)?;
    let stamp = CommittedTime::new(hlc, reader.u64()?).map_err(|_| E::Malformed)?;
    Ok(RawHeader {
        operation_tag: 11,
        tenant,
        group,
        recovery,
        request,
        intent,
        stamp,
    })
}

pub(super) fn prepare(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedCatalogAckRequest<'_>, E> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(E::ByteLimit);
    }
    let owned = encoded
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(4096))
        .ok_or(E::LengthOverflow)?;
    if owned > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut reader = Reader::new(encoded, limits);
    let header = header(&mut reader)?;
    keys::scope_length(header.scope())?;
    let body_at = encoded.len() - reader.remaining();
    let command = CreditCatalogAckRequest {
        expected_head: reader.u64()?,
        up_to: reader.u64()?,
    };
    reader.finish()?;
    command.validate().map_err(|_| E::Malformed)?;
    let mut hash = Sha256::new();
    hash.update(INTENT);
    hash.update(&encoded[body_at..]);
    if hash.finalize().as_slice() != header.intent.digest() {
        return Err(E::Malformed);
    }
    let isolation = keys::scope(header.scope())?;
    let scope = RequestScope::new(
        TenantId::new(copy(header.tenant)?).map_err(|_| E::Malformed)?,
        GroupId::new(copy(header.group)?).map_err(|_| E::Malformed)?,
        RecoveryId::new(copy(header.recovery)?).map_err(|_| E::Malformed)?,
    );
    let request = RequestId::new(copy(header.request)?).map_err(|_| E::Malformed)?;
    Ok(PreparedCatalogAckRequest {
        encoded,
        scope,
        request,
        stamp: header.stamp,
        intent: header.intent,
        digest: PreparedDigest::new(Sha256::digest(encoded).into()),
        isolation,
        command,
        owned_bytes: owned,
    })
}
