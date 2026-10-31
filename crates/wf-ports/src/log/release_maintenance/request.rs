//! Complete bounded command/EOF/digest preflight before fallible header ownership.
use super::{C, CodecLimits, E, PreparedBytes, PreparedReleaseMaintenanceRequest as P};
use crate::log::{
    common::RawHeader,
    keys,
    reader::{Reader, copy},
    writer::Writer,
};
use sha2::{Digest, Sha256};
use wf_domain::{
    governance::release::validate_release_completion_sequences,
    workflow::{
        control::PreparedDigest,
        request::{ClientIntent, RequestScope},
    },
};
use wf_types::{
    identity::{GroupId, RecoveryId, RequestId, TenantId},
    version::{CommittedTime, HlcTimestamp},
};
const INTENT: &[u8] = b"wf-release-maintenance-intent-v1\0";

pub(super) fn intent(command: &C) -> Result<ClientIntent, E> {
    command.validate().map_err(|_| E::Malformed)?;
    let mut hash = Sha256::new();
    hash.update(INTENT);
    match command {
        C::Acknowledge {
            expected_head,
            up_to,
        } => {
            hash.update([0]);
            hash.update(expected_head.to_be_bytes());
            hash.update(up_to.to_be_bytes());
        }
        C::MarkCompleted { sequences } => {
            hash.update([1]);
            hash.update(
                u16::try_from(sequences.len())
                    .map_err(|_| E::ItemLimit)?
                    .to_be_bytes(),
            );
            for sequence in sequences {
                hash.update(sequence.to_be_bytes());
            }
        }
    }
    Ok(ClientIntent::workflow_release_maintenance(
        hash.finalize().into(),
    ))
}
pub(super) fn encode(
    scope: &RequestScope,
    id: &RequestId,
    stamp: CommittedTime,
    command: &C,
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
    write_header(&mut measure, scope, id.as_bytes(), stamp, intent)?;
    body(&mut measure, command)?;
    if measure.len().checked_add(4096).ok_or(E::LengthOverflow)? > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut out = Writer::allocate(measure.len(), limits.admission().decision_bytes())?;
    write_header(&mut out, scope, id.as_bytes(), stamp, intent)?;
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
    id: &[u8],
    stamp: CommittedTime,
    intent: ClientIntent,
) -> Result<(), E> {
    out.write(b"FWCP\0\x01\x10")?;
    out.raw(scope.tenant().as_bytes(), 8192)?;
    for bytes in [scope.group().as_bytes(), scope.recovery().as_bytes(), id] {
        out.raw(bytes, 256)?;
    }
    out.u16(18)?;
    out.write(intent.digest())?;
    out.u64(stamp.hlc().physical().get())?;
    out.u16(stamp.hlc().logical())?;
    out.u64(stamp.wall().get())
}
fn body(out: &mut Writer<'_>, command: &C) -> Result<(), E> {
    match command {
        C::Acknowledge {
            expected_head,
            up_to,
        } => {
            out.byte(0)?;
            out.u64(*expected_head)?;
            out.u64(*up_to)
        }
        C::MarkCompleted { sequences } => {
            out.byte(1)?;
            out.u16(u16::try_from(sequences.len()).map_err(|_| E::ItemLimit)?)?;
            for sequence in sequences {
                out.u64(*sequence)?;
            }
            Ok(())
        }
    }
}
pub(super) fn header<'a>(reader: &mut Reader<'a>) -> Result<RawHeader<'a>, E> {
    if reader.take(4)? != b"FWCP" {
        return Err(E::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(E::UnsupportedVersion);
    }
    if reader.byte()? != 16 {
        return Err(E::UnsupportedTag);
    }
    let tenant = reader.nonempty(8192)?;
    let group = reader.nonempty(256)?;
    let recovery = reader.nonempty(256)?;
    let request = reader.nonempty(256)?;
    if reader.u16()? != 18 {
        return Err(E::UnsupportedVersion);
    }
    let intent = ClientIntent::workflow_release_maintenance(reader.fixed()?);
    let hlc = HlcTimestamp::new(reader.u64()?, reader.u16()?).map_err(|_| E::Malformed)?;
    let stamp = CommittedTime::new(hlc, reader.u64()?).map_err(|_| E::Malformed)?;
    Ok(RawHeader {
        operation_tag: 16,
        tenant,
        group,
        recovery,
        request,
        intent,
        stamp,
    })
}
fn scan_body(reader: &mut Reader<'_>) -> Result<usize, E> {
    match reader.byte()? {
        0 => {
            C::Acknowledge {
                expected_head: reader.u64()?,
                up_to: reader.u64()?,
            }
            .validate()
            .map_err(|_| E::Malformed)?;
            Ok(0)
        }
        1 => {
            let count = usize::from(reader.u16()?);
            if !(1..=256).contains(&count) {
                return Err(E::ItemLimit);
            }
            if count > reader.remaining() / 8 {
                return Err(E::Malformed);
            }
            let mut sequences = [0_u64; 256];
            for sequence in &mut sequences[..count] {
                *sequence = reader.u64()?;
            }
            validate_release_completion_sequences(&sequences[..count]).map_err(|_| E::Malformed)?;
            Ok(count)
        }
        _ => Err(E::UnsupportedTag),
    }
}
fn read_body(reader: &mut Reader<'_>) -> Result<C, E> {
    match reader.byte()? {
        0 => Ok(C::Acknowledge {
            expected_head: reader.u64()?,
            up_to: reader.u64()?,
        }),
        1 => {
            let count = usize::from(reader.u16()?);
            if !(1..=256).contains(&count) {
                return Err(E::ItemLimit);
            }
            let mut sequences = Vec::new();
            sequences
                .try_reserve_exact(count)
                .map_err(|_| E::Allocation)?;
            for _ in 0..count {
                sequences.push(reader.u64()?);
            }
            Ok(C::MarkCompleted { sequences })
        }
        _ => Err(E::UnsupportedTag),
    }
}
pub(super) fn prepare(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<P<'_>, E> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(E::ByteLimit);
    }
    let mut reader = Reader::new(encoded, limits);
    let header = header(&mut reader)?;
    let isolation_size = keys::scope_length(header.scope())?;
    let body_at = encoded.len() - reader.remaining();
    let count = scan_body(&mut reader)?;
    reader.finish()?;
    let mut hash = Sha256::new();
    hash.update(INTENT);
    hash.update(&encoded[body_at..]);
    if hash.finalize().as_slice() != header.intent.digest() {
        return Err(E::Malformed);
    }
    let owned = [
        encoded.len(),
        4096,
        isolation_size,
        count.checked_mul(8).ok_or(E::LengthOverflow)?,
        header.tenant.len(),
        header.group.len(),
        header.recovery.len(),
        header.request.len(),
    ]
    .into_iter()
    .try_fold(0_usize, usize::checked_add)
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
    let mut reader = Reader::new(encoded, limits);
    reader.take(body_at)?;
    let command = read_body(&mut reader)?;
    reader.finish()?;
    Ok(P {
        encoded,
        scope,
        request,
        intent: header.intent,
        stamp: header.stamp,
        digest: PreparedDigest::new(Sha256::digest(encoded).into()),
        isolation,
        command,
    })
}

pub(super) fn verify<'a>(
    encoded: &'a [u8],
    scope: &RequestScope,
    stamp: CommittedTime,
    command: &C,
    limits: CodecLimits,
) -> Result<(&'a [u8], ClientIntent, PreparedDigest), E> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(E::ByteLimit);
    }
    let header = header(&mut Reader::new(encoded, limits))?;
    if header.tenant != scope.tenant().as_bytes()
        || header.group != scope.group().as_bytes()
        || header.recovery != scope.recovery().as_bytes()
        || header.stamp != stamp
    {
        return Err(E::Malformed);
    }
    keys::scope_length(header.scope())?;
    let intent = intent(command)?;
    let mut out = Writer::verify(encoded, limits.admission().decision_bytes())?;
    write_header(&mut out, scope, header.request, stamp, intent)?;
    body(&mut out, command)?;
    out.finish_verification()?;
    Ok((
        header.request,
        intent,
        PreparedDigest::new(Sha256::digest(encoded).into()),
    ))
}
