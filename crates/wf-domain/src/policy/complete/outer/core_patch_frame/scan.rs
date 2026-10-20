//! One paid borrowed walk over complete frame structure and original Patch grammar.

use wf_types::version::{CommittedTime, HlcTimestamp};

use super::super::super::encoding::reader::Reader;
use super::super::super::{
    CompletePolicyEncodingError, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};
use super::{CompletePolicyRequestError, ScannedCorePatchFrame};

pub(super) fn scan_frame<'a>(
    encoded: &'a [u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(ScannedCorePatchFrame<'a>, &'a [u8]), CompletePolicyRequestError> {
    let (scanned, inner_tenant) = {
        let mut reader = Reader::new(encoded, limits, charge);
        reader.node()?;
        if reader.take(4)? != b"FWCP" {
            return Err(CompletePolicyEncodingError::Malformed.into());
        }
        if reader.u16()? != 1 {
            return Err(CompletePolicyEncodingError::UnsupportedVersion.into());
        }
        if reader.byte()? != 5 {
            return Err(CompletePolicyEncodingError::UnsupportedTag.into());
        }
        let tenant = reader.raw(8_192)?;
        tenant
            .first()
            .ok_or(CompletePolicyEncodingError::Malformed)?;
        let group = reader.raw(256)?;
        group
            .first()
            .ok_or(CompletePolicyEncodingError::Malformed)?;
        let recovery = reader.raw(256)?;
        recovery
            .first()
            .ok_or(CompletePolicyEncodingError::Malformed)?;
        let isolation = tenant
            .len()
            .checked_add(group.len())
            .ok_or(CompletePolicyRequestError::ScopeLimit)?;
        let isolation = isolation
            .checked_add(recovery.len())
            .ok_or(CompletePolicyRequestError::ScopeLimit)?;
        let isolation = isolation
            .checked_add(18)
            .ok_or(CompletePolicyRequestError::ScopeLimit)?;
        if isolation > 8_192 {
            return Err(CompletePolicyRequestError::ScopeLimit);
        }
        let request = reader.raw(256)?;
        request
            .first()
            .ok_or(CompletePolicyEncodingError::Malformed)?;
        if reader.u16()? != 2 {
            return Err(CompletePolicyEncodingError::UnsupportedVersion.into());
        }
        let intent_digest = reader
            .take(32)?
            .try_into()
            .or(Err(CompletePolicyEncodingError::Malformed))?;
        let hlc = HlcTimestamp::new(reader.u64()?, reader.u16()?)
            .or(Err(CompletePolicyEncodingError::Malformed))?;
        let stamp = CommittedTime::new(hlc, reader.u64()?)
            .or(Err(CompletePolicyEncodingError::Malformed))?;
        reader.node()?;
        let declared = reader.u32()?;
        if declared > 1_048_576 {
            return Err(CompletePolicyEncodingError::ByteLimit.into());
        }
        let n = usize::try_from(declared).or(Err(PolicySchemaError::LengthOverflow))?;
        let start = reader.position();
        let end = start
            .checked_add(n)
            .ok_or(PolicySchemaError::LengthOverflow)?;
        if end > encoded.len() {
            return Err(CompletePolicyEncodingError::Malformed.into());
        }
        if end < encoded.len() {
            return Err(CompletePolicyEncodingError::TrailingBytes.into());
        }
        let original_intent = &encoded[start..end];
        let (inner_tenant, state_changes) = {
            let mut intent = reader.region(start..end)?;
            let facts = scan_intent(&mut intent)?;
            intent.finish()?;
            facts
        };
        let scanned = ScannedCorePatchFrame {
            encoded,
            original_intent,
            tenant,
            group,
            recovery,
            request,
            stamp,
            intent_digest,
            state_changes,
        };
        (scanned, inner_tenant)
    };
    Ok((scanned, inner_tenant))
}

fn scan_intent<'a>(
    reader: &mut Reader<'a, '_>,
) -> Result<(&'a [u8], usize), CompletePolicyEncodingError> {
    reader.node()?;
    if reader.take(33)? != b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0" {
        return Err(CompletePolicyEncodingError::Malformed);
    }
    let tenant = reader.raw(8_192)?;
    if tenant.is_empty() {
        return Err(CompletePolicyEncodingError::Malformed);
    }
    if scan_optional(reader)? {
        reader.u64()?;
    }
    reader.node()?;
    reader.raw(262_144)?;
    if scan_field_patch(reader)? == 1 {
        scan_version(reader)?;
    }
    if scan_field_patch(reader)? == 1 {
        reader.u64()?;
    }
    if scan_field_patch(reader)? == 1 {
        scan_retry(reader)?;
    }
    if scan_field_patch(reader)? == 1 {
        scan_retention(reader)?;
    }
    let count = scan_state_changes(reader)?;
    Ok((tenant, count))
}

fn scan_field_patch(reader: &mut Reader<'_, '_>) -> Result<u8, CompletePolicyEncodingError> {
    let tag = reader.byte()?;
    match tag {
        0 | 2 => Ok(tag),
        1 => {
            reader.node()?;
            Ok(tag)
        }
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    }
}

fn scan_optional(reader: &mut Reader<'_, '_>) -> Result<bool, CompletePolicyEncodingError> {
    match reader.byte()? {
        0 => Ok(false),
        1 => {
            reader.node()?;
            Ok(true)
        }
        _ => Err(CompletePolicyEncodingError::InvalidOptionality),
    }
}

fn scan_version(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    match reader.byte()? {
        0 => {
            reader.raw(262_144)?;
        }
        1 => {
            reader.u64()?;
        }
        _ => return Err(CompletePolicyEncodingError::UnsupportedTag),
    }
    Ok(())
}

fn scan_retry(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    reader.node()?;
    if scan_optional(reader)? {
        reader.u16()?;
    }
    if scan_optional(reader)? {
        scan_backoff(reader)?;
    }
    if scan_optional(reader)? {
        reader.raw(262_144)?;
    }
    Ok(())
}

fn scan_backoff(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    reader.node()?;
    if scan_optional(reader)? && reader.byte()? > 3 {
        return Err(CompletePolicyEncodingError::UnsupportedTag);
    }
    if scan_optional(reader)? {
        reader.u64()?;
    }
    if scan_optional(reader)? {
        reader.u64()?;
    }
    if scan_optional(reader)? {
        reader.byte()?;
    }
    Ok(())
}

fn scan_retention(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    reader.node()?;
    if scan_optional(reader)? {
        reader.u64()?;
    }
    if scan_optional(reader)? {
        reader.u32()?;
    }
    Ok(())
}

fn scan_state_changes(reader: &mut Reader<'_, '_>) -> Result<usize, CompletePolicyEncodingError> {
    let count = reader.count(usize::MAX)?;
    let mut previous = None;
    for _ in 0..count {
        reader.node()?;
        let key = reader.raw(262_144)?;
        if let Some(previous) = previous {
            reader.keys(previous, key, 262_144)?;
        }
        match reader.byte()? {
            0 => {}
            1 => scan_state_patch(reader)?,
            _ => return Err(CompletePolicyEncodingError::UnsupportedTag),
        }
        previous = Some(key);
    }
    Ok(count)
}

fn scan_state_patch(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    reader.node()?;
    if scan_field_patch(reader)? == 1 && reader.byte()? > 1 {
        return Err(CompletePolicyEncodingError::UnsupportedTag);
    }
    if scan_field_patch(reader)? == 1 {
        scan_retry(reader)?;
    }
    if scan_field_patch(reader)? == 1 {
        scan_retention(reader)?;
    }
    Ok(())
}
