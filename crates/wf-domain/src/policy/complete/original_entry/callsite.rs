//! Distinct public Policy reads and raw `PolicyCommand` byte-reference preparation.

use std::fmt;

use super::{
    Error, OriginalEtfSafety, OriginalPolicyEntry, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, decode_original_complete_policy_entry, overflow, pay,
};

/// Supplied original Policy read-result classes; this enum performs no I/O.
#[derive(Clone, Copy, Debug)]
pub enum OriginalPolicyReadValue<'encoded> {
    /// Original nil/absent read.
    Absent,
    /// Original encoded stored value.
    Binary(&'encoded [u8]),
    /// A nonbinary stored value.
    Other,
    /// A shard-unavailable read.
    Unavailable,
    /// An error or invalid reader result.
    ReadFailed,
}

/// Operational original current/read errors, never incoming caller rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalPolicyReadError {
    /// Source decoding or atom-type binding failed.
    Corrupt,
    /// The supplied read originated from an unavailable shard.
    ShardUnavailable,
    /// The supplied reader errored or returned an invalid result.
    ReadFailed,
    /// A private operational profile, work, reservation or runtime restriction.
    Encoding(Error),
}

impl fmt::Display for OriginalPolicyReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Corrupt => formatter.write_str("ERR flow policy is corrupt"),
            Self::ShardUnavailable => formatter.write_str("ERR flow policy shard not available"),
            Self::ReadFailed => formatter.write_str("ERR flow policy read failed"),
            Self::Encoding(error) => fmt::Display::fmt(error, formatter),
        }
    }
}
impl std::error::Error for OriginalPolicyReadError {}

/// Applies the lower original Policy read/type clauses to supplied raw results.
///
/// Absence returns None, without synthesized policy bytes. Empty binary requested
/// types are allowed here; front command admission belongs to its actual caller.
///
/// # Errors
/// Preserves supplied read origin, source corruption and all operational reader errors.
pub fn read_original_complete_policy_entry<'encoded>(
    value: OriginalPolicyReadValue<'encoded>,
    requested_type: &[u8],
    safety: &OriginalEtfSafety<'_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<OriginalPolicyEntry<'encoded>>, OriginalPolicyReadError> {
    if charge.profile() != limits {
        return Err(OriginalPolicyReadError::Encoding(Error::Schema(
            PolicySchemaError::ProfileMismatch,
        )));
    }
    pay(charge, 128).map_err(OriginalPolicyReadError::Encoding)?;
    let raw = match value {
        OriginalPolicyReadValue::Absent => return Ok(None),
        OriginalPolicyReadValue::Other => return Err(OriginalPolicyReadError::Corrupt),
        OriginalPolicyReadValue::Unavailable => {
            return Err(OriginalPolicyReadError::ShardUnavailable);
        }
        OriginalPolicyReadValue::ReadFailed => return Err(OriginalPolicyReadError::ReadFailed),
        OriginalPolicyReadValue::Binary(raw) => raw,
    };
    let entry = decode_original_complete_policy_entry(raw, safety, limits, charge)
        .map_err(OriginalPolicyReadError::Encoding)?
        .ok_or(OriginalPolicyReadError::Corrupt)?;
    let length = requested_type
        .len()
        .checked_add(entry.workflow_type().map_or(0, <[u8]>::len))
        .ok_or_else(|| OriginalPolicyReadError::Encoding(overflow()))?;
    pay(
        charge,
        128 + u64::try_from(length).map_err(|_| OriginalPolicyReadError::Encoding(overflow()))?,
    )
    .map_err(OriginalPolicyReadError::Encoding)?;
    if entry.workflow_type() != Some(requested_type) {
        return Err(OriginalPolicyReadError::Corrupt);
    }
    Ok(Some(entry))
}

/// Prepares raw original reference bytes, with prospective SHA and metadata work.
///
/// The caller hashes only this returned Vec. No accepted Entry, public debit or
/// paid-token input exists; every call freshly executes the same raw read once.
///
/// # Errors
/// Returns no bytes on any read, cap, debit or fallible-reservation error.
pub fn prepare_original_complete_policy_reference_bytes(
    value: OriginalPolicyReadValue<'_>,
    requested_type: &[u8],
    safety: &OriginalEtfSafety<'_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(u64, Vec<u8>), OriginalPolicyReadError> {
    let entry = read_original_complete_policy_entry(value, requested_type, safety, limits, charge)?;
    let (generation, bytes) = if let Some(entry) = entry {
        let bytes = copy(entry.encoded(), charge).map_err(OriginalPolicyReadError::Encoding)?;
        (entry.generation(), bytes)
    } else {
        (
            0,
            fallback(requested_type, charge).map_err(OriginalPolicyReadError::Encoding)?,
        )
    };
    pay(
        charge,
        128 + u64::try_from(bytes.len())
            .map_err(|_| OriginalPolicyReadError::Encoding(overflow()))?,
    )
    .map_err(OriginalPolicyReadError::Encoding)?;
    pay(charge, 128).map_err(OriginalPolicyReadError::Encoding)?;
    Ok((generation, bytes))
}

fn copy(bytes: &[u8], charge: &mut PolicySchemaCharge) -> Result<Vec<u8>, Error> {
    pay(
        charge,
        128 + u64::try_from(bytes.len()).map_err(|_| overflow())?,
    )?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes.len())
        .map_err(|_| Error::AllocationFailure)?;
    output.extend_from_slice(bytes);
    Ok(output)
}

fn fallback(query: &[u8], charge: &mut PolicySchemaCharge) -> Result<Vec<u8>, Error> {
    let length = query.len().checked_add(37).ok_or_else(overflow)?;
    let measured = u64::try_from(length)
        .map_err(|_| overflow())?
        .checked_add(768)
        .ok_or_else(overflow)?;
    pay(charge, measured)?;
    if length > 263_168 {
        return Err(Error::ByteLimit);
    }
    pay(charge, 128 + u64::try_from(length).map_err(|_| overflow())?)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| Error::AllocationFailure)?;
    pay(charge, measured)?;
    bytes.extend_from_slice(
        b"\x83\x68\x03\x77\x0eflow_policy_v1\x61\x00\x74\x00\x00\x00\x01\x77\x04type\x6d",
    );
    bytes.extend_from_slice(
        &u32::try_from(query.len())
            .map_err(|_| overflow())?
            .to_be_bytes(),
    );
    bytes.extend_from_slice(query);
    Ok(bytes)
}
