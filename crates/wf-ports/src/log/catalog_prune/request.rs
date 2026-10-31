//! Complete borrowed inspection/digest verification before header ownership.
use super::{CodecLimits, E, PreparedBytes, PreparedCatalogPruneRequest};
use crate::log::{
    common::RawHeader,
    keys,
    reader::{Reader, copy},
    writer::Writer,
};
use sha2::{Digest, Sha256};
use wf_domain::{
    governance::catalog::CatalogPruneRequest as C,
    workflow::{
        control::PreparedDigest,
        request::{ClientIntent, RequestScope},
    },
};
use wf_types::{
    identity::{GroupId, RecoveryId, RequestId, TenantId},
    version::{CommittedTime, HlcTimestamp},
};
const INTENT: &[u8] = b"wf-governance-catalog-prune-intent-v1\0";

pub(super) fn intent(command: C<'_>) -> Result<ClientIntent, E> {
    command.validate().map_err(|_| E::Malformed)?;
    let mut hash = Sha256::new();
    hash.update(INTENT);
    hash.update(
        u32::try_from(command.catalog.len())
            .map_err(|_| E::LengthOverflow)?
            .to_be_bytes(),
    );
    hash.update(command.catalog);
    hash.update(
        u32::try_from(command.member.len())
            .map_err(|_| E::LengthOverflow)?
            .to_be_bytes(),
    );
    hash.update(command.member);
    hash.update(crate::state::encode_catalog_member_revision(
        command.expected,
    ));
    Ok(ClientIntent::governance_catalog_prune(
        hash.finalize().into(),
    ))
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
    out.write(b"FWCP\0\x01\x0e")?;
    out.raw(scope.tenant().as_bytes(), 8192)?;
    for part in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        request.as_bytes(),
    ] {
        out.raw(part, 256)?;
    }
    out.u16(16)?;
    out.write(intent)?;
    out.u64(stamp.hlc().physical().get())?;
    out.u16(stamp.hlc().logical())?;
    out.u64(stamp.wall().get())
}
fn body(out: &mut Writer<'_>, command: C<'_>) -> Result<(), E> {
    out.raw(command.catalog, 65535)?;
    out.raw(command.member, 65535)?;
    out.write(&crate::state::encode_catalog_member_revision(
        command.expected,
    ))
}
pub(super) fn header<'a>(reader: &mut Reader<'a>) -> Result<RawHeader<'a>, E> {
    if reader.take(4)? != b"FWCP" {
        return Err(E::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(E::UnsupportedVersion);
    }
    if reader.byte()? != 14 {
        return Err(E::UnsupportedTag);
    }
    let tenant = reader.nonempty(8192)?;
    let group = reader.nonempty(256)?;
    let recovery = reader.nonempty(256)?;
    let request = reader.nonempty(256)?;
    if reader.u16()? != 16 {
        return Err(E::UnsupportedVersion);
    }
    let intent = ClientIntent::governance_catalog_prune(reader.fixed()?);
    let hlc = HlcTimestamp::new(reader.u64()?, reader.u16()?).map_err(|_| E::Malformed)?;
    let stamp = CommittedTime::new(hlc, reader.u64()?).map_err(|_| E::Malformed)?;
    Ok(RawHeader {
        operation_tag: 14,
        tenant,
        group,
        recovery,
        request,
        intent,
        stamp,
    })
}
fn read_body<'a>(reader: &mut Reader<'a>) -> Result<C<'a>, E> {
    let catalog = reader.raw(65535)?;
    let member = reader.nonempty(65535)?;
    let expected = crate::state::inspect_catalog_member_revision(reader.take(62)?)?;
    Ok(C {
        catalog,
        member,
        expected,
    })
}
pub(super) fn prepare(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedCatalogPruneRequest<'_>, E> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(E::ByteLimit);
    }
    let mut reader = Reader::new(encoded, limits);
    let header = header(&mut reader)?;
    let isolation_bytes = keys::scope_length(header.scope())?;
    let body_at = encoded.len() - reader.remaining();
    let command = read_body(&mut reader)?;
    reader.finish()?;
    let mut hash = Sha256::new();
    hash.update(INTENT);
    hash.update(&encoded[body_at..]);
    if hash.finalize().as_slice() != header.intent.digest() {
        return Err(E::Malformed);
    }
    let owned = [
        2048,
        isolation_bytes,
        header.tenant.len(),
        header.group.len(),
        header.recovery.len(),
        header.request.len(),
    ]
    .into_iter()
    .try_fold(encoded.len(), usize::checked_add)
    .ok_or(E::LengthOverflow)?;
    if owned > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let isolation = keys::scope(header.scope())?;
    let scope = RequestScope::new(
        TenantId::new(copy(header.tenant)?).map_err(|_| E::Malformed)?,
        GroupId::new(copy(header.group)?).map_err(|_| E::Malformed)?,
        RecoveryId::new(copy(header.recovery)?).map_err(|_| E::Malformed)?,
    );
    let request = RequestId::new(copy(header.request)?).map_err(|_| E::Malformed)?;
    Ok(PreparedCatalogPruneRequest {
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
