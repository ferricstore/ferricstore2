//! Complete framing/checksum verification shared by full and reference reads.
use super::super::{
    budget::{Budget, add},
    model::{OriginalQueryRecordError as E, OriginalQueryRecordLimits},
};
use super::{cursor::Cursor, model::OriginalQueryReadLimits};
pub(super) struct Frame<'a> {
    pub(super) key: &'a [u8],
    pub(super) id: &'a [u8],
    pub(super) version: u64,
    pub(super) expiry: u64,
    pub(super) scope: Option<&'a [u8]>,
    pub(super) locator: &'a [u8],
    pub(super) metadata: &'a [u8],
}
pub(super) fn budget(l: OriginalQueryReadLimits, encoded: &[u8], key: &[u8]) -> Result<Budget, E> {
    if l.maximum_input_bytes == 0 {
        return Err(E::InvalidLimits);
    }
    let b = Budget::new(OriginalQueryRecordLimits {
        maximum_output_bytes: l.maximum_output_bytes,
        maximum_work_units: l.maximum_work_units,
        maximum_temporary_bytes: l.maximum_temporary_bytes,
    })?;
    if add(encoded.len(), key.len())? > l.maximum_input_bytes {
        return Err(E::Limit);
    }
    Ok(b)
}
pub(super) fn parse<'a>(encoded: &'a [u8], key: &'a [u8], b: &mut Budget) -> Result<Frame<'a>, E> {
    if encoded.len() > 32768 || key.is_empty() || key.len() > 65535 {
        return Err(E::InvalidRecord);
    }
    b.spend(add(key.len(), 49)?)?;
    let id = crate::blob::parse_original_state_key(key)
        .filter(|id| id.len() <= 65483)
        .ok_or(E::InvalidRecord)?;
    if encoded.get(..5) != Some(b"\xf7FQR\x01") {
        return Err(E::InvalidRecord);
    }
    let checksum = u32::from_be_bytes(
        encoded
            .get(5..9)
            .ok_or(E::InvalidRecord)?
            .try_into()
            .map_err(|_| E::InvalidRecord)?,
    );
    let body = encoded.get(9..).ok_or(E::InvalidRecord)?;
    b.spend(add(key.len(), body.len())?.checked_mul(8).ok_or(E::Limit)?)?;
    if super::super::row::checksum(key, body) != checksum {
        return Err(E::InvalidRecord);
    }
    let mut c = Cursor::new(body);
    let version = c.varint(b)?;
    let expiry = c.varint(b)?;
    let scope_len = c.u16(b)?;
    let locator_len = c.u16(b)?;
    let metadata_len = c.u16(b)?;
    if scope_len > 8192
        || locator_len == 0
        || locator_len > 104
        || add(scope_len, metadata_len)? > 32629
    {
        return Err(E::InvalidRecord);
    }
    let scope = c.take(scope_len, b)?;
    let locator = c.take(locator_len, b)?;
    let metadata = c.take(metadata_len, b)?;
    c.finish()?;
    Ok(Frame {
        key,
        id,
        version,
        expiry,
        scope: (!scope.is_empty()).then_some(scope),
        locator,
        metadata,
    })
}
