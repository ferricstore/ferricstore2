//! Explicit captured metadata caller operands beside a verified stored-value batch.
mod frame;
mod model;
use super::{CodecError, DecodedRequest, PreparedBytes};
use super::{
    prepare_workflow_stored_value_batch,
    reader::{self, Reader},
    writer::Writer,
};
pub use model::{MetadataBatchInput, MetadataBatchLimits};
use sha2::{Digest, Sha256};
use std::mem::size_of;
use wf_domain::workflow::request::ClientIntent;

/// Encodes actual inner caller identity and ordered original metadata operands.
/// Generated preparation context must not be supplied as a metadata caller operand.
/// # Errors
/// Rejects inner/profile/framing or independent prospective resource limits.
pub fn encode_workflow_metadata_batch(
    input: MetadataBatchInput<'_>,
    limits: MetadataBatchLimits,
) -> Result<PreparedBytes, CodecError> {
    validate_limits(limits)?;
    let mut remaining = limits.maximum_work_units;
    spend(&mut remaining, 65_536)?;
    if input.metadata.is_empty() || input.metadata.len() > limits.codec.admission().batch_items() {
        return Err(CodecError::ItemLimit);
    }
    spend(
        &mut remaining,
        input
            .metadata
            .len()
            .checked_mul(4096)
            .ok_or(CodecError::LengthOverflow)?,
    )?;
    let mut measure = Writer::measure(limits.codec.admission().decision_bytes());
    frame::write(
        input,
        ClientIntent::workflow_metadata_batch([0; 32]),
        &mut measure,
        limits,
    )?;
    let size = measure
        .len()
        .checked_add(32)
        .ok_or(CodecError::LengthOverflow)?;
    if size > limits.codec.admission().decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    spend(
        &mut remaining,
        size.checked_mul(128).ok_or(CodecError::LengthOverflow)?,
    )?;
    let live = owner(input.metadata.len(), 0, size)?;
    let (decoded, used) = prepare_workflow_stored_value_batch(
        input.stored_batch,
        limits.codec,
        available(limits.maximum_owned_bytes, live)?,
        remaining,
    )?;
    spend(&mut remaining, used)?;
    check_count(&decoded, input.metadata.len())?;
    // Canonical option bytes are already laid out consecutively in the final frame.
    let mut output = Writer::allocate(size, limits.codec.admission().decision_bytes())?;
    frame::write(
        input,
        ClientIntent::workflow_metadata_batch([0; 32]),
        &mut output,
        limits,
    )?;
    let mut bytes = output.finish()?;
    let start = 14_usize
        .checked_add(input.stored_batch.len())
        .ok_or(CodecError::LengthOverflow)?;
    let end = bytes.len().checked_sub(34).ok_or(CodecError::Malformed)?;
    let intent = frame::intent(
        decoded.request.intent,
        input.metadata.len(),
        &bytes[start..end],
    )?;
    bytes[end + 2..].copy_from_slice(intent.digest());
    let footer: [u8; 32] = Sha256::digest(&bytes).into();
    bytes.extend_from_slice(&footer);
    Ok(PreparedBytes {
        digest: super::digest(&bytes),
        bytes,
    })
}

/// Verifies and owns one explicit intent22 command for the existing reducer.
/// Opaque metadata supplies no original-source or typed ETF validity promise.
/// # Errors
/// Rejects malformed/foreign profiles, caller digest or resource limits.
pub fn prepare_workflow_metadata_batch(
    encoded: &[u8],
    limits: MetadataBatchLimits,
) -> Result<(DecodedRequest<'_>, usize), CodecError> {
    validate_limits(limits)?;
    let mut remaining = outer_work(encoded.len(), limits)?;
    let parts = frame::scan(encoded, limits, &mut remaining)?;
    let live = owner(parts.count, parts.metadata_bytes, 0)?;
    let (mut decoded, used) = prepare_workflow_stored_value_batch(
        parts.inner,
        limits.codec,
        available(limits.maximum_owned_bytes, live)?,
        remaining,
    )?;
    spend(&mut remaining, used)?;
    check_count(&decoded, parts.count)?;
    let intent = frame::intent(decoded.request.intent, parts.count, parts.region)?;
    if intent != parts.expected {
        return Err(CodecError::Malformed);
    }
    let mut metadata = Vec::new();
    metadata
        .try_reserve_exact(parts.count)
        .map_err(|_| CodecError::Allocation)?;
    let mut reader = Reader::new(parts.region, limits.codec);
    for _ in 0..parts.count {
        metadata.push(reader.optional(|reader| {
            reader::copy(reader.raw(limits.codec.admission().decision_bytes())?)
        })?);
    }
    reader.finish()?;
    decoded.set_record_metadata_updates(metadata)?;
    decoded.request.intent = intent;
    decoded.record_batch_intent = Ok(intent);
    decoded.encoded = encoded;
    decoded.digest = super::digest(encoded);
    Ok((decoded, limits.maximum_work_units - remaining))
}

fn validate_limits(limits: MetadataBatchLimits) -> Result<(), CodecError> {
    if limits.maximum_owned_bytes < 4096 {
        return Err(CodecError::OwnedLimit);
    }
    if limits.maximum_work_units == 0 {
        return Err(CodecError::WorkLimit);
    }
    Ok(())
}
fn check_count(decoded: &DecodedRequest<'_>, count: usize) -> Result<(), CodecError> {
    if decoded
        .request
        .operation
        .mutation_items()
        .ok_or(CodecError::UnsupportedTag)?
        .len()
        != count
    {
        return Err(CodecError::ItemLimit);
    }
    Ok(())
}
fn owner(count: usize, metadata: usize, output: usize) -> Result<usize, CodecError> {
    4096_usize
        .checked_add(
            count
                .checked_mul(size_of::<Option<Vec<u8>>>())
                .ok_or(CodecError::LengthOverflow)?,
        )
        .and_then(|n| n.checked_add(metadata))
        .and_then(|n| n.checked_add(output))
        .ok_or(CodecError::LengthOverflow)
}
fn available(maximum: usize, held: usize) -> Result<usize, CodecError> {
    maximum
        .checked_sub(held)
        .filter(|n| *n != 0)
        .ok_or(CodecError::OwnedLimit)
}
fn outer_work(bytes: usize, limits: MetadataBatchLimits) -> Result<usize, CodecError> {
    if bytes > limits.codec.admission().decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    let mut remaining = limits.maximum_work_units;
    spend(&mut remaining, 65_536)?;
    spend(
        &mut remaining,
        bytes.checked_mul(128).ok_or(CodecError::LengthOverflow)?,
    )?;
    Ok(remaining)
}
fn spend(remaining: &mut usize, amount: usize) -> Result<(), CodecError> {
    *remaining = remaining.checked_sub(amount).ok_or(CodecError::WorkLimit)?;
    Ok(())
}

impl DecodedRequest<'_> {
    pub(in crate::log) fn set_record_metadata_updates(
        &mut self,
        updates: Vec<Option<Vec<u8>>>,
    ) -> Result<(), CodecError> {
        self.value_metadata
            .as_mut()
            .ok_or(CodecError::Malformed)?
            .captures = Some(updates);
        Ok(())
    }
    /// Borrows original opaque metadata operands in exact source-item order.
    #[must_use]
    pub fn record_metadata_updates(&self) -> Option<&[Option<Vec<u8>>]> {
        self.value_metadata.as_ref()?.captures.as_deref()
    }
    pub(in crate::log) fn take_record_metadata_updates(&mut self) -> Option<Vec<Option<Vec<u8>>>> {
        self.value_metadata.as_mut()?.captures.take()
    }
}
