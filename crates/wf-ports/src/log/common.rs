//! Fixed header and item fields shared by borrowed validation and owned decoding.

use wf_domain::workflow::request::ClientIntent;
use wf_types::version::{CommittedTime, HlcTimestamp, TimestampMillis};

use super::{CodecError, reader::Reader};

pub(super) struct RawHeader<'a> {
    pub(super) operation_tag: u8,
    pub(super) tenant: &'a [u8],
    pub(super) group: &'a [u8],
    pub(super) recovery: &'a [u8],
    pub(super) request: &'a [u8],
    pub(super) intent: ClientIntent,
    pub(super) stamp: CommittedTime,
}

impl RawHeader<'_> {
    pub(super) const fn scope(&self) -> [&[u8]; 3] {
        [self.tenant, self.group, self.recovery]
    }
}

pub(super) struct RawItem<'a> {
    pub(super) partition: &'a [u8],
    pub(super) id: &'a [u8],
    pub(super) priority: u8,
    pub(super) now: TimestampMillis,
    pub(super) tag: u8,
}

pub(super) fn header<'a>(reader: &mut Reader<'a>) -> Result<RawHeader<'a>, CodecError> {
    if reader.take(4)? != b"FWCP" {
        return Err(CodecError::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(CodecError::UnsupportedVersion);
    }
    let operation_tag = reader.byte()?;
    if !matches!(operation_tag, 1..=3) {
        return Err(CodecError::UnsupportedTag);
    }
    let tenant = reader.nonempty(8_192)?;
    let group = reader.nonempty(256)?;
    let recovery = reader.nonempty(256)?;
    let request = reader.nonempty(256)?;
    let version = reader.u16()?;
    let digest = reader.fixed()?;
    let intent = match (version, operation_tag) {
        (1, _) => ClientIntent::new(1, digest).map_err(|_| CodecError::UnsupportedVersion)?,
        (5, 1) => ClientIntent::record_batch(digest),
        _ => return Err(CodecError::UnsupportedVersion),
    };
    let hlc = HlcTimestamp::new(reader.u64()?, reader.u16()?).map_err(|_| CodecError::Malformed)?;
    let stamp = CommittedTime::new(hlc, reader.u64()?).map_err(|_| CodecError::Malformed)?;
    Ok(RawHeader {
        operation_tag,
        tenant,
        group,
        recovery,
        request,
        intent,
        stamp,
    })
}

pub(super) fn count(reader: &mut Reader<'_>) -> Result<usize, CodecError> {
    let count = usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)?;
    if count == 0 || count > reader.limits.admission.batch_items() {
        return Err(CodecError::ItemLimit);
    }
    // Each nonempty pair of IDs plus priority/time/tag consumes at least20 bytes.
    if count > reader.remaining() / 20 {
        return Err(CodecError::Malformed);
    }
    Ok(count)
}

pub(super) fn item<'a>(reader: &mut Reader<'a>) -> Result<RawItem<'a>, CodecError> {
    Ok(RawItem {
        partition: reader.nonempty(65_535)?,
        id: reader.nonempty(65_535)?,
        priority: reader.byte()?,
        now: TimestampMillis::new(reader.u64()?),
        tag: reader.byte()?,
    })
}
