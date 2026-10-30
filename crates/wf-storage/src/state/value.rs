//! Versioned core/checkpoint/history/batch5 values; no engine or durability.

use std::fmt;

use wf_ports::log::CodecError;

mod control;
mod history;
mod history_snapshot;
mod membership;
mod outcome;
mod record;
mod workflow_values;

pub use control::{decode_control, encode_control};
pub use history::{decode_history, encode_history};
pub use history_snapshot::{
    HistorySnapshotLimits, decode_history_snapshot, encode_history_snapshot,
    encode_history_snapshot_fields,
};
pub(crate) use history_snapshot::{
    binding_source_fields_in_layout, encode_record_in_layout, history_snapshot_encoded_size,
    inspect_history_snapshot, scan_record_in_layout,
};
pub(crate) use membership::inspect_membership;
pub use membership::{
    decode_applied_membership, encode_applied_membership, measure_applied_membership,
};
pub(crate) use outcome::outcome_policy_validation_bytes_in_layout;
pub use outcome::{
    ScannedRetainedOutcome, decode_outcome, encode_outcome, encode_outcome_profiled,
    encode_step_continue_outcome, outcome_policy_validation_bytes, scan_outcome,
    scan_outcome_profiled, scan_step_continue_outcome,
};
pub(crate) use outcome::{encode_outcome_in_layout, scan_outcome_in_layout};
pub use record::{
    ScannedWorkflowRecord, decode_record, encode_record, encode_record_fields,
    encode_record_snapshot, scan_record_point, scan_record_point_profiled, scan_record_snapshot,
};
pub(crate) use workflow_values::{
    ValueAlias, ValueBindingData, ValueSourceFields, binding_source_fields, decode_alias,
    decode_binding, encode_alias, encode_binding, validate_binding_fields, validate_stored_binary,
};

/// Operational persisted-value failure, never a retained business rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateValueError {
    /// The represented value or response version is unsupported.
    UnsupportedVersion,
    /// A magic, event, response or presence discriminator is unknown.
    UnsupportedTag,
    /// Complete framing is truncated or contains trailing bytes.
    Malformed,
    /// Checked sizes cannot fit the host or explicit wire width.
    LengthOverflow,
    /// Encoded bytes or decoded logical bytes exceed their explicit caller cap.
    Limit {
        /// Actual bytes in the unit of the exceeded encoded or logical cap.
        bytes: usize,
        /// Explicit maximum in the same unit as `bytes`.
        maximum: usize,
    },
    /// A newly introduced fallible variable reservation failed.
    Allocation,
    /// A complete membership count/address/owner profile was exhausted.
    MembershipLimit,
    /// A healthy response exceeds the actual captured response profile.
    ProfileLimit,
    /// Valid original ETF needs another captured portable atom/profile context.
    UnsupportedPolicyProfile,
    /// Namespace, typed value, status or positive profile is invalid.
    InvalidValue,
}

impl fmt::Display for StateValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Limit { bytes, maximum } => {
                write!(formatter, "state value {bytes} exceeds {maximum}")
            }
            Self::UnsupportedVersion => formatter.write_str("unsupported state value version"),
            Self::UnsupportedTag => formatter.write_str("unsupported state value tag"),
            Self::Malformed => formatter.write_str("malformed state value"),
            Self::LengthOverflow => formatter.write_str("state value length overflow"),
            Self::Allocation => formatter.write_str("state value allocation failed"),
            Self::MembershipLimit => formatter.write_str("state membership profile limit"),
            Self::ProfileLimit => formatter.write_str("state response profile limit"),
            Self::UnsupportedPolicyProfile => {
                formatter.write_str("unsupported original policy profile")
            }
            Self::InvalidValue => formatter.write_str("invalid state value"),
        }
    }
}
impl std::error::Error for StateValueError {}

impl From<CodecError> for StateValueError {
    fn from(error: CodecError) -> Self {
        match error {
            CodecError::UnsupportedVersion => Self::UnsupportedVersion,
            CodecError::UnsupportedTag | CodecError::InvalidOptionality => Self::UnsupportedTag,
            CodecError::Malformed | CodecError::TrailingBytes => Self::Malformed,
            CodecError::LengthOverflow => Self::LengthOverflow,
            CodecError::Allocation => Self::Allocation,
            _ => Self::InvalidValue,
        }
    }
}

pub(super) fn start(
    bytes: &[u8],
    magic: [u8; 4],
    maximum: usize,
) -> Result<usize, StateValueError> {
    let (offset, version) = start_versioned(bytes, magic, maximum)?;
    if version != 1 {
        return Err(StateValueError::UnsupportedVersion);
    }
    Ok(offset)
}

pub(super) fn start_versioned(
    bytes: &[u8],
    magic: [u8; 4],
    maximum: usize,
) -> Result<(usize, u16), StateValueError> {
    if maximum == 0 {
        return Err(StateValueError::InvalidValue);
    }
    if bytes.len() > maximum {
        return Err(StateValueError::Limit {
            bytes: bytes.len(),
            maximum,
        });
    }
    let mut offset = 0;
    if take(bytes, &mut offset, 4)? != magic.as_slice() {
        return Err(StateValueError::UnsupportedTag);
    }
    let version = read_u16(bytes, &mut offset)?;
    Ok((offset, version))
}

pub(super) fn take<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
    length: usize,
) -> Result<&'a [u8], StateValueError> {
    let end = offset
        .checked_add(length)
        .ok_or(StateValueError::LengthOverflow)?;
    let value = bytes.get(*offset..end).ok_or(StateValueError::Malformed)?;
    *offset = end;
    Ok(value)
}

pub(super) fn raw<'a>(bytes: &'a [u8], offset: &mut usize) -> Result<&'a [u8], StateValueError> {
    let length =
        usize::try_from(read_u32(bytes, offset)?).map_err(|_| StateValueError::LengthOverflow)?;
    take(bytes, offset, length)
}

pub(super) fn read_u8(bytes: &[u8], offset: &mut usize) -> Result<u8, StateValueError> {
    Ok(take(bytes, offset, 1)?[0])
}
pub(super) fn read_u16(bytes: &[u8], offset: &mut usize) -> Result<u16, StateValueError> {
    Ok(u16::from_be_bytes(
        take(bytes, offset, 2)?
            .try_into()
            .map_err(|_| StateValueError::Malformed)?,
    ))
}
pub(super) fn read_u32(bytes: &[u8], offset: &mut usize) -> Result<u32, StateValueError> {
    Ok(u32::from_be_bytes(
        take(bytes, offset, 4)?
            .try_into()
            .map_err(|_| StateValueError::Malformed)?,
    ))
}
pub(super) fn read_u64(bytes: &[u8], offset: &mut usize) -> Result<u64, StateValueError> {
    Ok(u64::from_be_bytes(
        take(bytes, offset, 8)?
            .try_into()
            .map_err(|_| StateValueError::Malformed)?,
    ))
}
pub(super) fn read_u128(bytes: &[u8], offset: &mut usize) -> Result<u128, StateValueError> {
    Ok(u128::from_be_bytes(
        take(bytes, offset, 16)?
            .try_into()
            .map_err(|_| StateValueError::Malformed)?,
    ))
}

pub(super) fn flag(bytes: &[u8], offset: &mut usize) -> Result<bool, StateValueError> {
    match read_u8(bytes, offset)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(StateValueError::UnsupportedTag),
    }
}

pub(super) fn finish(bytes: &[u8], offset: usize) -> Result<(), StateValueError> {
    if offset == bytes.len() {
        Ok(())
    } else {
        Err(StateValueError::Malformed)
    }
}

pub(super) fn copy(bytes: &[u8]) -> Result<Vec<u8>, StateValueError> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(bytes.len())
        .map_err(|_| StateValueError::Allocation)?;
    result.extend_from_slice(bytes);
    Ok(result)
}

pub(super) fn raw_size(bytes: &[u8]) -> Result<usize, StateValueError> {
    u32::try_from(bytes.len()).map_err(|_| StateValueError::LengthOverflow)?;
    bytes
        .len()
        .checked_add(4)
        .ok_or(StateValueError::LengthOverflow)
}

pub(super) fn add(total: &mut usize, length: usize) -> Result<(), StateValueError> {
    *total = total
        .checked_add(length)
        .ok_or(StateValueError::LengthOverflow)?;
    Ok(())
}

pub(super) fn allocate(
    size: usize,
    maximum: usize,
    magic: [u8; 4],
) -> Result<Vec<u8>, StateValueError> {
    allocate_versioned(size, maximum, magic, 1)
}

pub(super) fn allocate_versioned(
    size: usize,
    maximum: usize,
    magic: [u8; 4],
    version: u16,
) -> Result<Vec<u8>, StateValueError> {
    if maximum == 0 || size < 6 {
        return Err(StateValueError::InvalidValue);
    }
    if size > maximum {
        return Err(StateValueError::Limit {
            bytes: size,
            maximum,
        });
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(size)
        .map_err(|_| StateValueError::Allocation)?;
    result.extend_from_slice(&magic);
    result.extend_from_slice(&version.to_be_bytes());
    Ok(result)
}

pub(super) fn put_raw(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), StateValueError> {
    output.extend_from_slice(
        &u32::try_from(bytes.len())
            .map_err(|_| StateValueError::LengthOverflow)?
            .to_be_bytes(),
    );
    output.extend_from_slice(bytes);
    Ok(())
}

pub(super) fn canonical_history_position(bytes: &[u8]) -> Result<(u64, u64), StateValueError> {
    let separator = bytes
        .iter()
        .position(|byte| *byte == b'-')
        .ok_or(StateValueError::InvalidValue)?;
    Ok((
        canonical_integer(&bytes[..separator])?,
        canonical_integer(&bytes[separator + 1..])?,
    ))
}

fn canonical_integer(bytes: &[u8]) -> Result<u64, StateValueError> {
    if bytes.is_empty() || (bytes.len() > 1 && bytes[0] == b'0') {
        return Err(StateValueError::InvalidValue);
    }
    bytes.iter().try_fold(0_u64, |value, byte| {
        if !byte.is_ascii_digit() {
            return Err(StateValueError::InvalidValue);
        }
        value
            .checked_mul(10)
            .and_then(|value| value.checked_add(u64::from(byte - b'0')))
            .ok_or(StateValueError::InvalidValue)
    })
}
