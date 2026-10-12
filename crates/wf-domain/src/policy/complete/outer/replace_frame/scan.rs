//! Original Replace grammar and separately paid borrowed list uniqueness.

use std::collections::BTreeSet;

use wf_types::version::{CommittedTime, HlcTimestamp};

use super::super::super::admission::charge_schema;
use super::super::super::encoding::reader::Reader;
use super::super::super::encoding::scan::scan_data_fields;
use super::super::super::{
    CompletePolicyData, CompletePolicyEncodingError, GovernanceData, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits,
};
use super::{CompletePolicyRequestError, ScannedCompletePolicyReplaceFrame};

pub(super) fn scan_frame<'a>(
    encoded: &'a [u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(ScannedCompletePolicyReplaceFrame<'a>, &'a [u8]), CompletePolicyRequestError> {
    let (scanned, inner_tenant) = {
        let mut reader = Reader::new(encoded, limits, charge);
        reader.node()?;
        if reader.take(4)? != b"FWCP" {
            return Err(CompletePolicyEncodingError::Malformed.into());
        }
        if reader.u16()? != 1 {
            return Err(CompletePolicyEncodingError::UnsupportedVersion.into());
        }
        if reader.byte()? != 6 {
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
        let inner_tenant = {
            let mut intent = reader.region(start..end)?;
            let tenant = scan_intent(&mut intent)?;
            intent.finish()?;
            tenant
        };
        let scanned = ScannedCompletePolicyReplaceFrame {
            encoded,
            original_intent,
            tenant,
            group,
            recovery,
            request,
            stamp,
            intent_digest,
        };
        (scanned, inner_tenant)
    };
    Ok((scanned, inner_tenant))
}

fn scan_intent<'a>(reader: &mut Reader<'a, '_>) -> Result<&'a [u8], CompletePolicyEncodingError> {
    reader.node()?;
    if reader.take(30)? != b"WF-INTENT\0COMPLETE-REPLACE\0V2\0" {
        return Err(CompletePolicyEncodingError::Malformed);
    }
    let tenant = reader.raw(8_192)?;
    if tenant.is_empty() {
        return Err(CompletePolicyEncodingError::Malformed);
    }
    reader.optional(Reader::u64)?;
    reader.node()?;
    scan_data_fields(reader)?;
    Ok(tenant)
}

pub(in crate::policy::complete::outer) fn check_replace_frame_lists(
    data: &CompletePolicyData,
    charge: &mut PolicySchemaCharge,
) -> Result<(), CompletePolicyRequestError> {
    charge_schema(charge, 128, 0)?;
    for state in data.states.values() {
        charge_schema(charge, 128, 0)?;
        if let Some(governance) = &state.governance {
            check_replace_governance_lists(governance, charge)?;
        }
    }
    if let Some(names) = &data.indexed_attributes {
        check_replace_frame_names(names, charge)?;
    }
    if let Some(governance) = &data.governance {
        check_replace_governance_lists(governance, charge)?;
    }
    Ok(())
}

fn check_replace_governance_lists(
    data: &GovernanceData,
    charge: &mut PolicySchemaCharge,
) -> Result<(), CompletePolicyRequestError> {
    charge_schema(charge, 128, 0)?;
    if let Some(effects) = &data.effects {
        charge_schema(charge, 128, 0)?;
        for names in [&effects.allowed, &effects.denied].into_iter().flatten() {
            check_replace_frame_names(names, charge)?;
        }
    }
    if let Some(circuits) = &data.circuits {
        charge_schema(charge, 128, 0)?;
        for circuit in circuits.values() {
            charge_schema(charge, 128, 0)?;
            if let Some(names) = &circuit.error_classes {
                check_replace_frame_names(names, charge)?;
            }
        }
    }
    if let Some(approvals) = &data.approvals {
        charge_schema(charge, 128, 0)?;
        for names in [&approvals.effects, &approvals.states, &approvals.assignees]
            .into_iter()
            .flatten()
        {
            check_replace_frame_names(names, charge)?;
        }
    }
    Ok(())
}

fn check_replace_frame_names(
    names: &[Vec<u8>],
    charge: &mut PolicySchemaCharge,
) -> Result<(), CompletePolicyRequestError> {
    charge_schema(charge, 128, 0)?;
    let scratch = u64::try_from(names.len())
        .ok()
        .and_then(|n| n.checked_mul(64))
        .ok_or(PolicySchemaError::LengthOverflow)?;
    charge_schema(charge, 0, scratch)?;
    let mut seen = BTreeSet::new();
    for name in names {
        let work = u64::try_from(name.len())
            .ok()
            .and_then(|n| n.checked_add(128))
            .ok_or(PolicySchemaError::LengthOverflow)?;
        charge_schema(charge, work, 0)?;
        if !seen.insert(name.as_slice()) {
            return Err(CompletePolicyEncodingError::Malformed.into());
        }
    }
    Ok(())
}
