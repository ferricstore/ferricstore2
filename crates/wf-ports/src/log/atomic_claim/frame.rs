//! One canonical caller span and separate operational/payment data.

use super::super::{common::RawHeader, reader::copy};
use super::super::{governed_claim::payment, keys, reader::Reader, writer::Writer};
use super::model::PreparedAtomicClaim;
use super::{
    AtomicClaimOptions, CodecError as E, CodecLimits, CommittedTime, NormalizedClaimDue,
    PreparedBytes, RequestId, RequestScope, caller,
};
use sha2::{Digest, Sha256};
use wf_domain::workflow::control::PreparedDigest;
use wf_domain::{claim::caller::ClaimCallerLimits, workflow::request::ClientIntent};
use wf_types::{
    identity::{GroupId, RecoveryId, TenantId},
    version::HlcTimestamp,
};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(in crate::log) enum FrameProfile {
    Prepaid,
    PublicFunded,
}
impl FrameProfile {
    const fn operation(self) -> u8 {
        match self {
            Self::Prepaid => 18,
            Self::PublicFunded => 19,
        }
    }
    const fn intent_version(self) -> u16 {
        match self {
            Self::Prepaid => 20,
            Self::PublicFunded => 21,
        }
    }
    const fn intent(self, digest: [u8; 32]) -> ClientIntent {
        match self {
            Self::Prepaid => ClientIntent::atomic_claim(digest),
            Self::PublicFunded => ClientIntent::public_claim(digest),
        }
    }
}

const INTENT: &[u8] = b"wf-atomic-claim-original-caller-v1\0";

/// Borrows original parent recovery routing facts; complete preparation is required.
/// # Errors
/// Refuses other profiles, invalid headers or encoded-byte admission.
pub fn atomic_claim_recovery_header(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<(u16, [&[u8]; 3], CommittedTime), E> {
    recovery_profile(encoded, limits, FrameProfile::Prepaid)
}

pub(in crate::log) fn recovery_profile(
    encoded: &[u8],
    limits: CodecLimits,
    profile: FrameProfile,
) -> Result<(u16, [&[u8]; 3], CommittedTime), E> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(E::ByteLimit);
    }
    let header = read_header(&mut Reader::new(encoded, limits), profile)?;
    Ok((
        match profile {
            FrameProfile::Prepaid => 20,
            FrameProfile::PublicFunded => 21,
        },
        [header.tenant, header.group, header.recovery],
        header.stamp,
    ))
}

/// Encodes every normalized caller field except the separate block attempt.
/// # Errors
/// Refuses complete frame, encoded/owner limits or malformed original payment.
pub fn encode_atomic_claim_request(
    scope: &RequestScope,
    request: &RequestId,
    stamp: CommittedTime,
    call: &NormalizedClaimDue<'_>,
    options: AtomicClaimOptions<'_>,
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedBytes, E> {
    encode_profile(
        scope,
        request,
        stamp,
        call,
        (options, None),
        limits,
        maximum_owned,
    )
}

pub(in crate::log) fn encode_profile(
    scope: &RequestScope,
    request: &RequestId,
    stamp: CommittedTime,
    call: &NormalizedClaimDue<'_>,
    (options, batch): (AtomicClaimOptions<'_>, Option<[u8; 16]>),
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedBytes, E> {
    let profile = if batch.is_some() {
        FrameProfile::PublicFunded
    } else {
        FrameProfile::Prepaid
    };
    if batch.is_some() && options.payment.is_some() {
        return Err(E::Malformed);
    }
    keys::scope_length([
        scope.tenant().as_bytes(),
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
    ])?;
    let maximum = limits.admission().decision_bytes();
    let mut measured = Writer::measure(maximum);
    header(&mut measured, scope, request, stamp, profile)?;
    caller::write(&mut measured, call, limits)?;
    write_options(&mut measured, options, batch)?;
    let required = measured.len().checked_add(4096).ok_or(E::LengthOverflow)?;
    if required > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut out = Writer::allocate(measured.len(), maximum)?;
    let digest_at = header(&mut out, scope, request, stamp, profile)?;
    let start = out.len();
    caller::write(&mut out, call, limits)?;
    let end = out.len();
    let options_at = out.len();
    write_options(&mut out, options, batch)?;
    let mut bytes = out.finish()?;
    let mut reader = Reader::new(bytes.get(options_at..).ok_or(E::Malformed)?, limits);
    // Decode the actual option bytes before any duplicate-set ownership.
    reader.u16()?;
    reader.u32()?;
    reader.u32()?;
    reader.u32()?;
    let paid = if batch.is_some() {
        reader.take(16)?;
        None
    } else {
        reader.optional(|reader| payment::read(reader, &bytes))?
    };
    reader.finish()?;
    if let Some(paid) = paid {
        payment::validate(paid, maximum_owned - required)?;
    }
    let digest = match profile {
        FrameProfile::Prepaid => intent_digest(
            scope.tenant().as_bytes(),
            bytes.get(start..end).ok_or(E::Malformed)?,
        )?,
        FrameProfile::PublicFunded => {
            *crate::log::public_claim::intent::prepare(scope.tenant().as_bytes(), call, maximum)?
                .digest()
        }
    };
    bytes
        .get_mut(digest_at..digest_at + 32)
        .ok_or(E::Malformed)?
        .copy_from_slice(&digest);
    Ok(PreparedBytes {
        digest: PreparedDigest::new(Sha256::digest(&bytes).into()),
        bytes,
    })
}

fn intent_digest(tenant: &[u8], caller: &[u8]) -> Result<[u8; 32], E> {
    let mut hash = Sha256::new();
    hash.update(INTENT);
    hash.update(
        u32::try_from(tenant.len())
            .map_err(|_| E::LengthOverflow)?
            .to_be_bytes(),
    );
    hash.update(tenant);
    hash.update(caller);
    Ok(hash.finalize().into())
}

fn write_options(
    out: &mut Writer<'_>,
    options: AtomicClaimOptions<'_>,
    batch: Option<[u8; 16]>,
) -> Result<(), E> {
    if options.token_version != 1 {
        return Err(E::UnsupportedVersion);
    }
    out.u16(options.token_version)?;
    out.u32(options.first_ordinal)?;
    out.u32(u32::try_from(options.scan_budget).map_err(|_| E::LengthOverflow)?)?;
    out.u32(u32::try_from(options.head_budget).map_err(|_| E::LengthOverflow)?)?;
    match batch {
        Some(batch) => out.write(&batch),
        None => out.optional(options.payment.as_ref(), |out, paid| {
            payment::write(out, *paid)
        }),
    }
}

fn header(
    out: &mut Writer<'_>,
    scope: &RequestScope,
    request: &RequestId,
    stamp: CommittedTime,
    profile: FrameProfile,
) -> Result<usize, E> {
    out.write(b"FWCP\0\x01")?;
    out.byte(profile.operation())?;
    out.raw(scope.tenant().as_bytes(), 8192)?;
    for bytes in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        request.as_bytes(),
    ] {
        out.raw(bytes, 256)?;
    }
    out.u16(profile.intent_version())?;
    let at = out.len();
    out.write(&[0; 32])?;
    out.u64(stamp.hlc().physical().get())?;
    out.u16(stamp.hlc().logical())?;
    out.u64(stamp.wall().get())?;
    Ok(at)
}

/// Completely scans and admits original frame ownership before caller decoding.
/// # Errors
/// Refuses complete syntax/intent/canonicality, original payment or configured bounds.
pub fn prepare_atomic_claim_request(
    encoded: &[u8],
    limits: CodecLimits,
    caller_limits: ClaimCallerLimits,
    maximum_owned: usize,
) -> Result<PreparedAtomicClaim<'_>, E> {
    prepare_profile(
        encoded,
        limits,
        caller_limits,
        maximum_owned,
        FrameProfile::Prepaid,
    )
}

pub(in crate::log) fn prepare_profile(
    encoded: &[u8],
    limits: CodecLimits,
    caller_limits: ClaimCallerLimits,
    maximum_owned: usize,
    profile: FrameProfile,
) -> Result<PreparedAtomicClaim<'_>, E> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(E::ByteLimit);
    }
    let mut reader = Reader::new(encoded, limits);
    let raw = read_header(&mut reader, profile)?;
    keys::scope_length(raw.scope())?;
    let start = encoded.len() - reader.remaining();
    let (_, count) = caller::read(&mut reader, false)?;
    let end = encoded.len() - reader.remaining();
    let token_version = reader.u16()?;
    if token_version != 1 {
        return Err(E::UnsupportedVersion);
    }
    let first_ordinal = reader.u32()?;
    let scan_budget = usize::try_from(reader.u32()?).map_err(|_| E::LengthOverflow)?;
    let head_budget = usize::try_from(reader.u32()?).map_err(|_| E::LengthOverflow)?;
    let batch = if profile == FrameProfile::PublicFunded {
        Some(reader.fixed()?)
    } else {
        None
    };
    let paid = if profile == FrameProfile::Prepaid {
        reader.optional(|r| payment::read(r, encoded))?
    } else {
        None
    };
    reader.finish()?;
    let caller_bytes = encoded.get(start..end).ok_or(E::Malformed)?;
    if profile == FrameProfile::Prepaid
        && intent_digest(raw.tenant, caller_bytes)? != *raw.intent.digest()
    {
        return Err(E::Malformed);
    }
    let bound = encoded
        .len()
        .checked_mul(4)
        .and_then(|n| count.checked_mul(128).and_then(|m| n.checked_add(m)))
        .and_then(|n| n.checked_add(4096))
        .ok_or(E::LengthOverflow)?;
    if bound > maximum_owned {
        return Err(E::OwnedLimit);
    }
    if let Some(paid) = paid {
        payment::validate(paid, maximum_owned - bound)?;
    }
    let mut reader = Reader::new(caller_bytes, limits);
    let (caller, _) = caller::read(&mut reader, true)?;
    reader.finish()?;
    let original = caller.normalize(caller_limits)?;
    let mut check = Writer::verify(caller_bytes, limits.admission().decision_bytes())?;
    caller::write(&mut check, &original, limits)?;
    check.finish_verification()?;
    if profile == FrameProfile::PublicFunded
        && crate::log::public_claim::intent::prepare(
            raw.tenant,
            &original,
            limits.admission().decision_bytes(),
        )?
        .digest()
            != raw.intent.digest()
    {
        return Err(E::Malformed);
    }
    drop(original);
    Ok(PreparedAtomicClaim {
        encoded,
        scope: RequestScope::new(
            TenantId::new(copy(raw.tenant)?).map_err(|_| E::Malformed)?,
            GroupId::new(copy(raw.group)?).map_err(|_| E::Malformed)?,
            RecoveryId::new(copy(raw.recovery)?).map_err(|_| E::Malformed)?,
        ),
        request: RequestId::new(copy(raw.request)?).map_err(|_| E::Malformed)?,
        intent: raw.intent,
        stamp: raw.stamp,
        digest: PreparedDigest::new(Sha256::digest(encoded).into()),
        owned_bytes: bound,
        isolation: keys::scope(raw.scope())?,
        caller,
        caller_bytes,
        token_version,
        first_ordinal,
        scan_budget,
        head_budget,
        payment: paid,
        funding_batch: batch,
    })
}

fn read_header<'a>(reader: &mut Reader<'a>, profile: FrameProfile) -> Result<RawHeader<'a>, E> {
    if reader.take(4)? != b"FWCP" {
        return Err(E::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(E::UnsupportedVersion);
    }
    if reader.byte()? != profile.operation() {
        return Err(E::UnsupportedTag);
    }
    let tenant = reader.nonempty(8192)?;
    let group = reader.nonempty(256)?;
    let recovery = reader.nonempty(256)?;
    let request = reader.nonempty(256)?;
    if reader.u16()? != profile.intent_version() {
        return Err(E::UnsupportedVersion);
    }
    let intent = profile.intent(reader.fixed()?);
    let hlc = HlcTimestamp::new(reader.u64()?, reader.u16()?).map_err(|_| E::Malformed)?;
    let stamp = CommittedTime::new(hlc, reader.u64()?).map_err(|_| E::Malformed)?;
    Ok(RawHeader {
        operation_tag: profile.operation(),
        tenant,
        group,
        recovery,
        request,
        intent,
        stamp,
    })
}
