//! Fixed known governance grammar during borrowed preflight.

use super::super::{CompletePolicyEncodingError, Reader};
use super::scan_names;

pub(super) fn scan_governance(
    reader: &mut Reader<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    reader.node()?;
    reader.optional(|reader| match reader.byte()? {
        0..=2 => Ok(()),
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    })?;
    reader.optional(scan_effects)?;
    reader.optional(scan_audit)?;
    reader.optional(scan_limits)?;
    reader.optional(scan_budgets)?;
    reader.optional(scan_circuits)?;
    reader.optional(scan_approvals)?;
    Ok(())
}

fn scan_effects(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    reader.node()?;
    reader.optional(|reader| scan_names(reader, 1000, 65535))?;
    reader.optional(|reader| scan_names(reader, 1000, 65535))?;
    reader.optional(|reader| match reader.byte()? {
        0 | 1 => Ok(()),
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    })?;
    Ok(())
}

fn scan_audit(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    reader.node()?;
    reader.optional(|reader| match reader.byte()? {
        0 | 1 => Ok(()),
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    })?;
    reader.optional(|reader| match reader.byte()? {
        0..=2 => Ok(()),
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    })?;
    Ok(())
}

fn scan_limits(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    let count = reader.count(1000)?;
    let mut previous = None;
    for _ in 0..count {
        reader.node()?;
        let key = reader.raw(65535)?;
        if let Some(previous) = previous {
            reader.keys(previous, key, 65535)?;
        }
        previous = Some(key);
        scan_limit(reader)?;
    }
    Ok(())
}

fn scan_budgets(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    let count = reader.count(1000)?;
    let mut previous = None;
    for _ in 0..count {
        reader.node()?;
        let key = reader.raw(65535)?;
        if let Some(previous) = previous {
            reader.keys(previous, key, 65535)?;
        }
        previous = Some(key);
        scan_budget(reader)?;
    }
    Ok(())
}

fn scan_circuits(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    let count = reader.count(1000)?;
    let mut previous = None;
    for _ in 0..count {
        reader.node()?;
        let key = reader.raw(65535)?;
        if let Some(previous) = previous {
            reader.keys(previous, key, 65535)?;
        }
        previous = Some(key);
        scan_circuit(reader)?;
    }
    Ok(())
}

fn scan_limit(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    reader.node()?;
    reader.u64()?;
    reader.optional(scan_enforcement)?;
    reader.optional(Reader::u64)?;
    Ok(())
}

fn scan_budget(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    reader.node()?;
    reader.u64()?;
    reader.u64()?;
    reader.optional(scan_enforcement)?;
    reader.optional(Reader::u64)?;
    reader.optional(|reader| reader.raw(65535))?;
    Ok(())
}

fn scan_circuit(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    reader.node()?;
    reader.u64()?;
    reader.u64()?;
    reader.optional(Reader::u64)?;
    reader.optional(Reader::u64)?;
    reader.optional(Reader::byte)?;
    reader.optional(Reader::u64)?;
    reader.optional(Reader::u64)?;
    reader.optional(Reader::u64)?;
    reader.optional(|reader| scan_names(reader, 1000, 256))?;
    Ok(())
}

fn scan_approvals(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    reader.node()?;
    reader.optional(|reader| scan_names(reader, 1000, 65535))?;
    reader.optional(|reader| scan_names(reader, 1000, 65535))?;
    reader.optional(|reader| scan_names(reader, 1000, 65535))?;
    reader.optional(Reader::u64)?;
    Ok(())
}

fn scan_enforcement(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    match reader.byte()? {
        0 | 1 => Ok(()),
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    }
}
