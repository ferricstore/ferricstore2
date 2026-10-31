//! Complete borrowed inspection precedes the one reference-vector allocation.

use super::model::PreparedCatalogMembershipRequest;
use crate::log::{
    CodecError as E, CodecLimits, PreparedBytes,
    common::RawHeader,
    keys,
    reader::{Reader, copy},
    writer::Writer,
};
use sha2::{Digest, Sha256};
use wf_domain::{
    governance::catalog::{CatalogMembershipAction as A, CatalogMembershipChange as C},
    workflow::{
        control::PreparedDigest,
        request::{ClientIntent, RequestScope},
    },
};
use wf_types::{
    identity::{GroupId, RecoveryId, RequestId, TenantId},
    version::{CommittedTime, HlcTimestamp},
};

const INTENT: &[u8] = b"wf-governance-catalog-intent-v1\0";

pub(super) fn intent(command: C<'_>) -> Result<ClientIntent, E> {
    command.validate(65_535).map_err(|_| E::Malformed)?;
    let mut hash = Sha256::new();
    hash.update(INTENT);
    hash.update([tag(command.action)]);
    hash.update(
        u32::try_from(command.catalog.len())
            .map_err(|_| E::LengthOverflow)?
            .to_be_bytes(),
    );
    hash.update(command.catalog);
    hash.update(
        u16::try_from(command.members.len())
            .map_err(|_| E::LengthOverflow)?
            .to_be_bytes(),
    );
    for member in command.members {
        hash.update(
            u32::try_from(member.len())
                .map_err(|_| E::LengthOverflow)?
                .to_be_bytes(),
        );
        hash.update(member);
    }
    Ok(ClientIntent::governance_catalog(hash.finalize().into()))
}

const fn tag(action: A) -> u8 {
    match action {
        A::Register => 0,
        A::Unregister => 1,
    }
}
fn action(reader: &mut Reader<'_>) -> Result<A, E> {
    match reader.byte()? {
        0 => Ok(A::Register),
        1 => Ok(A::Unregister),
        _ => Err(E::UnsupportedTag),
    }
}

pub(super) fn encode(
    scope: &RequestScope,
    request: &RequestId,
    stamp: CommittedTime,
    command: C<'_>,
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedBytes, E> {
    let intent = intent(command)?;
    keys::scope_length([
        scope.tenant().as_bytes(),
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
    ])?;
    let mut measure = Writer::measure(limits.admission().decision_bytes());
    write_header(&mut measure, scope, request, stamp, intent.digest())?;
    body(&mut measure, command)?;
    if measure.len().checked_add(2048).ok_or(E::LengthOverflow)? > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut out = Writer::allocate(measure.len(), limits.admission().decision_bytes())?;
    write_header(&mut out, scope, request, stamp, intent.digest())?;
    body(&mut out, command)?;
    let bytes = out.finish()?;
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
    out.write(b"FWCP\0\x01\x0c")?;
    out.raw(scope.tenant().as_bytes(), 8192)?;
    for part in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        request.as_bytes(),
    ] {
        out.raw(part, 256)?;
    }
    out.u16(14)?;
    out.write(intent)?;
    out.u64(stamp.hlc().physical().get())?;
    out.u16(stamp.hlc().logical())?;
    out.u64(stamp.wall().get())
}
fn body(out: &mut Writer<'_>, command: C<'_>) -> Result<(), E> {
    out.byte(tag(command.action))?;
    out.raw(command.catalog, 65_535)?;
    out.u16(u16::try_from(command.members.len()).map_err(|_| E::LengthOverflow)?)?;
    for member in command.members {
        out.raw(member, 65_535)?;
    }
    Ok(())
}

pub(super) fn header<'a>(reader: &mut Reader<'a>) -> Result<RawHeader<'a>, E> {
    if reader.take(4)? != b"FWCP" {
        return Err(E::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(E::UnsupportedVersion);
    }
    if reader.byte()? != 12 {
        return Err(E::UnsupportedTag);
    }
    let tenant = reader.nonempty(8192)?;
    let group = reader.nonempty(256)?;
    let recovery = reader.nonempty(256)?;
    let request = reader.nonempty(256)?;
    if reader.u16()? != 14 {
        return Err(E::UnsupportedVersion);
    }
    let intent = ClientIntent::governance_catalog(reader.fixed()?);
    let hlc = HlcTimestamp::new(reader.u64()?, reader.u16()?).map_err(|_| E::Malformed)?;
    let stamp = CommittedTime::new(hlc, reader.u64()?).map_err(|_| E::Malformed)?;
    Ok(RawHeader {
        operation_tag: 12,
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
) -> Result<PreparedCatalogMembershipRequest<'_>, E> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(E::ByteLimit);
    }
    let owned = encoded
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(8192))
        .ok_or(E::LengthOverflow)?;
    if owned > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut reader = Reader::new(encoded, limits);
    let header = header(&mut reader)?;
    keys::scope_length(header.scope())?;
    let body_at = encoded.len() - reader.remaining();
    let action = action(&mut reader)?;
    let catalog = reader.raw(65_535)?;
    let count = usize::from(reader.u16()?);
    if count > 256 {
        return Err(E::ItemLimit);
    }
    for _ in 0..count {
        let member = reader.raw(65_535)?;
        if action == A::Register && member.is_empty() {
            return Err(E::Malformed);
        }
    }
    reader.finish()?;
    let mut hash = Sha256::new();
    hash.update(INTENT);
    hash.update(&encoded[body_at..]);
    if hash.finalize().as_slice() != header.intent.digest() {
        return Err(E::Malformed);
    }
    // Revisit only the fully verified borrowed body, then allocate reference slots.
    let mut body = Reader::new(&encoded[body_at..], limits);
    body.byte()?;
    body.raw(65_535)?;
    body.u16()?;
    let mut members = Vec::new();
    members
        .try_reserve_exact(count)
        .map_err(|_| E::Allocation)?;
    for _ in 0..count {
        members.push(body.raw(65_535)?);
    }
    body.finish()?;
    let isolation = keys::scope(header.scope())?;
    let scope = RequestScope::new(
        TenantId::new(copy(header.tenant)?).map_err(|_| E::Malformed)?,
        GroupId::new(copy(header.group)?).map_err(|_| E::Malformed)?,
        RecoveryId::new(copy(header.recovery)?).map_err(|_| E::Malformed)?,
    );
    let request = RequestId::new(copy(header.request)?).map_err(|_| E::Malformed)?;
    Ok(PreparedCatalogMembershipRequest {
        encoded,
        scope,
        request,
        stamp: header.stamp,
        intent: header.intent,
        digest: PreparedDigest::new(Sha256::digest(encoded).into()),
        isolation,
        catalog,
        action,
        members,
        owned_bytes: owned,
    })
}
