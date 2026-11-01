//! Closed outer framing; original metadata remains explicitly opaque data.
use super::{MetadataBatchInput, MetadataBatchLimits, spend};
use crate::log::{CodecError, reader::Reader, writer::Writer};
use sha2::{Digest, Sha256};
use wf_domain::workflow::request::ClientIntent;

pub(super) struct MetadataFrame<'a> {
    pub(super) inner: &'a [u8],
    pub(super) region: &'a [u8],
    pub(super) count: usize,
    pub(super) expected: ClientIntent,
    pub(super) metadata_bytes: usize,
}

pub(super) fn validate_operand(bytes: &[u8]) -> Result<(), CodecError> {
    if bytes.starts_with(b"FSV2\x02\x83") {
        Ok(())
    } else {
        Err(CodecError::Malformed)
    }
}

pub(super) fn write(
    input: MetadataBatchInput<'_>,
    intent: ClientIntent,
    writer: &mut Writer<'_>,
    limits: MetadataBatchLimits,
) -> Result<(), CodecError> {
    if input.metadata.is_empty() || input.metadata.len() > limits.codec.admission().batch_items() {
        return Err(CodecError::ItemLimit);
    }
    let maximum = limits.codec.admission().decision_bytes();
    writer.write(b"FWMD")?;
    writer.u16(1)?;
    writer.raw(input.stored_batch, maximum)?;
    writer.u32(u32::try_from(input.metadata.len()).map_err(|_| CodecError::LengthOverflow)?)?;
    for value in input.metadata {
        writer.optional(*value, |writer, bytes| {
            validate_operand(bytes)?;
            writer.raw(bytes, maximum)
        })?;
    }
    writer.u16(22)?;
    writer.write(intent.digest())
}

pub(super) fn scan<'a>(
    encoded: &'a [u8],
    limits: MetadataBatchLimits,
    remaining: &mut usize,
) -> Result<MetadataFrame<'a>, CodecError> {
    let mut reader = Reader::new(encoded, limits.codec);
    if reader.take(4)? != b"FWMD" || reader.u16()? != 1 {
        return Err(CodecError::UnsupportedVersion);
    }
    let maximum = limits.codec.admission().decision_bytes();
    let inner = reader.nonempty(maximum)?;
    let count = usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)?;
    if count == 0 || count > limits.codec.admission().batch_items() {
        return Err(CodecError::ItemLimit);
    }
    spend(
        remaining,
        count.checked_mul(4096).ok_or(CodecError::LengthOverflow)?,
    )?;
    let start = encoded.len() - reader.remaining();
    let mut metadata_bytes = 0_usize;
    for _ in 0..count {
        reader.optional(|reader| {
            let bytes = reader.raw(maximum)?;
            validate_operand(bytes)?;
            metadata_bytes = metadata_bytes
                .checked_add(bytes.len())
                .ok_or(CodecError::LengthOverflow)?;
            Ok(())
        })?;
    }
    let region = &encoded[start..encoded.len() - reader.remaining()];
    if reader.u16()? != 22 {
        return Err(CodecError::UnsupportedVersion);
    }
    let expected = ClientIntent::workflow_metadata_batch(reader.fixed()?);
    let prefix = encoded.len() - reader.remaining();
    let footer: [u8; 32] = reader.fixed()?;
    let actual: [u8; 32] = Sha256::digest(&encoded[..prefix]).into();
    if footer != actual {
        return Err(CodecError::Malformed);
    }
    reader.finish()?;
    Ok(MetadataFrame {
        inner,
        region,
        count,
        expected,
        metadata_bytes,
    })
}

pub(super) fn intent(
    inner: ClientIntent,
    count: usize,
    region: &[u8],
) -> Result<ClientIntent, CodecError> {
    if inner.version() != 9 {
        return Err(CodecError::UnsupportedVersion);
    }
    let mut hash = Sha256::new();
    hash.update(b"WF-METADATA-CALLER\0V22\0");
    hash.update(inner.digest());
    hash.update(
        u32::try_from(count)
            .map_err(|_| CodecError::LengthOverflow)?
            .to_be_bytes(),
    );
    hash.update(region);
    Ok(ClientIntent::workflow_metadata_batch(
        hash.finalize().into(),
    ))
}
