//! Compact metadata binding; the core digest covers native identity and ordering.
use super::{
    StateValueError as E, ValueBindingData, allocate, decode_binding, encode_binding, finish,
    put_raw, raw, raw_size, read_u64, start, take,
};
use sha2::{Digest, Sha256};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::RequestScope;
use wf_ports::consensus::WorkflowValueDefaults;
use wf_types::identity::{PartitionId, RecordId};

pub(crate) struct RecordMetadataBinding<'a> {
    pub(crate) binding: ValueBindingData,
    pub(crate) version: u64,
    pub(crate) core_digest: [u8; 32],
    pub(crate) metadata: &'a [u8],
}

pub(crate) fn encoded_metadata_binding_size(data: &RecordMetadataBinding<'_>) -> Result<usize, E> {
    if data.version == 0 || !data.metadata.starts_with(b"FSV2\x02\x83") {
        return Err(E::InvalidValue);
    }
    let life = data.binding.lifecycle;
    let count = data
        .binding
        .versions
        .into_iter()
        .chain([
            life.birth.max_active_ms(),
            life.effective_ttl_ms,
            life.terminal_deadline
                .map(wf_types::version::TimestampMillis::get),
        ])
        .flatten()
        .count();
    let base = 149_usize
        .checked_add(count.checked_mul(8).ok_or(E::LengthOverflow)?)
        .ok_or(E::LengthOverflow)?;
    raw_size(data.metadata)?;
    86_usize
        .checked_add(base)
        .and_then(|n| n.checked_add(data.metadata.len()))
        .ok_or(E::LengthOverflow)
}

pub(crate) fn encode_metadata_binding(
    data: &RecordMetadataBinding<'_>,
    maximum: usize,
) -> Result<Vec<u8>, E> {
    let size = encoded_metadata_binding_size(data)?;
    if size > maximum {
        return Err(E::Limit {
            bytes: size,
            maximum,
        });
    }
    let base = encode_binding(data.binding, 197)?;
    let mut bytes = allocate(size, maximum, *b"FWMB")?;
    put_raw(&mut bytes, &base)?;
    bytes.extend_from_slice(&data.version.to_be_bytes());
    bytes.extend_from_slice(&data.core_digest);
    put_raw(&mut bytes, data.metadata)?;
    if bytes.len().checked_add(32) != Some(size) {
        return Err(E::InvalidValue);
    }
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    bytes.extend_from_slice(&digest);
    Ok(bytes)
}

pub(crate) fn decode_metadata_binding(
    bytes: &[u8],
    defaults: WorkflowValueDefaults,
) -> Result<RecordMetadataBinding<'_>, E> {
    let mut at = start(bytes, *b"FWMB", bytes.len())?;
    let base = raw(bytes, &mut at)?;
    if base.len() > 197 || !base.starts_with(b"FWVB\0\x01") {
        return Err(E::InvalidValue);
    }
    let version = read_u64(bytes, &mut at)?;
    let core_digest = take(bytes, &mut at, 32)?
        .try_into()
        .map_err(|_| E::Malformed)?;
    let metadata = raw(bytes, &mut at)?;
    let end = at;
    let expected = take(bytes, &mut at, 32)?;
    finish(bytes, at)?;
    let actual: [u8; 32] = Sha256::digest(&bytes[..end]).into();
    if actual != expected {
        return Err(E::InvalidValue);
    }
    let result = RecordMetadataBinding {
        binding: decode_binding(base, defaults)?,
        version,
        core_digest,
        metadata,
    };
    encoded_metadata_binding_size(&result)?;
    Ok(result)
}

/// Extracts the existing canonical core without copying another full image.
pub(crate) fn metadata_source_core(bytes: &[u8], history: bool) -> Result<&[u8], E> {
    let record = if history {
        let mut at = start(bytes, *b"FWHS", bytes.len())?;
        let record = raw(bytes, &mut at)?;
        raw(bytes, &mut at)?;
        finish(bytes, at)?;
        record
    } else {
        bytes
    };
    if !record.starts_with(b"FWSR\0\x02") {
        return Err(E::InvalidValue);
    }
    Ok(record)
}

pub(crate) fn validate_metadata_source(
    data: &RecordMetadataBinding<'_>,
    source: &[u8],
    scope: &RequestScope,
    identity: (&PartitionId, &RecordId),
    history: Option<&[u8]>,
    admission: AdmissionLimits,
    maximum: usize,
) -> Result<(), E> {
    let bytes = metadata_source_core(source, history.is_some())?;
    let record = super::record::scan_record_snapshot(
        bytes, scope, identity.0, identity.1, admission, maximum,
    )?;
    if let Some(event) = history {
        let scanned = super::history_snapshot::inspect_history_snapshot(
            source, scope, identity.0, identity.1, admission, maximum,
        )?;
        if scanned.event_id() != event {
            return Err(E::InvalidValue);
        }
    }
    let actual: [u8; 32] = Sha256::digest(bytes).into();
    let (kind, sequence) = record.capture_birth_fields();
    if actual != data.core_digest
        || data.version != record.version()
        || (kind == wf_domain::history::EventKind::Created
            && sequence != data.binding.lifecycle.incarnation)
    {
        return Err(E::InvalidValue);
    }
    Ok(())
}
