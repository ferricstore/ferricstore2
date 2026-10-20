//! Pure normalized V2 VALUE bytes through one parent-owned schema account.

use std::fmt;

use crate::policy::installation::{MAX_ENCODED_POLICY_BYTES, MAX_POLICY_GENERATION};

use super::admission::charge_schema;
use super::validation::admit_complete;
use super::{
    CompletePolicyCandidate, CompletePolicyData, CompletePolicyValue, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits,
};

pub(in crate::policy::complete) mod decode;
mod encode;
pub(in crate::policy::complete) mod reader;
pub(in crate::policy::complete) mod scan;
mod writer;

use reader::Reader;
use writer::Writer;

struct MeasuredPolicyV2<'v> {
    generation: u64,
    data: &'v CompletePolicyData,
    snapshot_len: usize,
    envelope_len: usize,
}

/// Normalized V2 content, wire, correspondence and allocation refusals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletePolicyEncodingError {
    /// Existing typed schema and operational work diagnosis.
    Schema(PolicySchemaError),
    /// The envelope or snapshot version is unsupported.
    UnsupportedVersion,
    /// An enum or boolean tag is unsupported.
    UnsupportedTag,
    /// A structurally valid original value needs a different portable safety profile.
    UnsupportedPolicyProfile,
    /// Reached source map order needs runtime facts outside the portable profile.
    UnsupportedSourceOrder,
    /// An optional field has an invalid discriminant.
    InvalidOptionality,
    /// The encoded structure is incomplete or noncanonical.
    Malformed,
    /// Bytes remain after the declared envelope or snapshot.
    TrailingBytes,
    /// An actual or declared encoded byte length exceeds its cap.
    ByteLimit,
    /// A collection count exceeds its field limit.
    ItemLimit,
    /// Existing encoded bytes disagree with the normalized value.
    EncodingMismatch,
    /// A selected fallible vector reservation failed.
    AllocationFailure,
}

impl fmt::Display for CompletePolicyEncodingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schema(error) => fmt::Display::fmt(error, formatter),
            Self::UnsupportedVersion => {
                formatter.write_str("unsupported complete policy encoding version")
            }
            Self::UnsupportedTag => formatter.write_str("unsupported complete policy encoding tag"),
            Self::UnsupportedPolicyProfile => {
                formatter.write_str("unsupported original policy safety profile")
            }
            Self::UnsupportedSourceOrder => {
                formatter.write_str("unsupported original policy source order")
            }
            Self::InvalidOptionality => formatter.write_str("invalid complete policy optionality"),
            Self::Malformed => formatter.write_str("malformed complete policy encoding"),
            Self::TrailingBytes => formatter.write_str("trailing complete policy bytes"),
            Self::ByteLimit => formatter.write_str("complete policy encoding byte limit exceeded"),
            Self::ItemLimit => formatter.write_str("complete policy encoding item limit exceeded"),
            Self::EncodingMismatch => formatter.write_str("complete policy encoding mismatch"),
            Self::AllocationFailure => {
                formatter.write_str("complete policy encoding allocation failure")
            }
        }
    }
}

impl std::error::Error for CompletePolicyEncodingError {}

/// Encodes admitted normalized V2 content, moving its original semantic owners.
///
/// The canonical bytes supply no namespace, hash, installation or source ETF proof.
/// Logical peak scratch excludes input construction, retained owners and native costs.
///
/// # Errors
/// Preserves profile/work diagnoses, maps current content to invalid authority, and
/// keeps new encoded-size and selected fallible reservation failures distinct.
pub fn prepare_complete_policy_value_v2(
    candidate: CompletePolicyCandidate,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyValue, CompletePolicyEncodingError> {
    let encoded = encode_complete_policy_candidate_v2(&candidate, limits, charge)?;
    Ok(CompletePolicyValue { candidate, encoded })
}

pub(super) fn encode_complete_policy_candidate_v2(
    candidate: &CompletePolicyCandidate,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<u8>, CompletePolicyEncodingError> {
    if charge.profile() != limits {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch,
        ));
    }
    if candidate.generation() == 0 || candidate.generation() > MAX_POLICY_GENERATION {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::InvalidAuthority,
        ));
    }
    admit_complete(candidate.data(), limits, charge, false).map_err(current_error)?;
    let measured = encode::measure_value(candidate.generation(), candidate.data(), limits, charge)?;
    encode::emit_value(&measured, limits, charge)
}

/// Re-admits current content and streams its V2 bytes against the existing owner.
///
/// No second encoded output is allocated. Success supplies no installed authority.
///
/// # Errors
/// Checks profile, generation and actual envelope length before current admission;
/// preserves operational errors and reports any subsequent byte disagreement.
pub fn verify_complete_policy_value_v2(
    value: &CompletePolicyValue,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), CompletePolicyEncodingError> {
    if charge.profile() != limits {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch,
        ));
    }
    if value.generation() == 0 || value.generation() > MAX_POLICY_GENERATION {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::InvalidAuthority,
        ));
    }
    if value.encoded().len() > MAX_ENCODED_POLICY_BYTES {
        return Err(CompletePolicyEncodingError::ByteLimit);
    }
    admit_complete(value.data(), limits, charge, false).map_err(current_error)?;
    let measured = encode::measure_value(value.generation(), value.data(), limits, charge)
        .map_err(|error| match error {
            CompletePolicyEncodingError::Schema(error) => current_error(error),
            error => error,
        })?;
    encode::compare_value(&measured, value.encoded(), limits, charge)
}

/// Decodes one canonical normalized V2 VALUE after complete borrowed preflight.
///
/// Structural refusal precedes semantic/envelope outputs. Later typed refusal drops
/// materialized owners; construction, destruction and native costs remain separate.
///
/// # Errors
/// Preserves ordered wire and typed schema refusals, profile/work diagnoses, and
/// selected fallible vector reservation failures without resetting the account.
pub fn decode_complete_policy_value_v2(
    encoded: &[u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyValue, CompletePolicyEncodingError> {
    decode_value(encoded, limits, charge, false)
}

/// Decodes the same complete value with explicit configured-state-cap attribution.
/// # Errors
/// Only the selected state count cap becomes a typed schema `StateBudget` refusal;
/// hard grammar/count limits, work, profile and allocation errors stay exact.
pub fn decode_complete_policy_value_v2_profiled(
    encoded: &[u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyValue, CompletePolicyEncodingError> {
    decode_value(encoded, limits, charge, true)
}
fn decode_value(
    encoded: &[u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    profiled: bool,
) -> Result<CompletePolicyValue, CompletePolicyEncodingError> {
    let candidate = decode_candidate(encoded, limits, charge, profiled)?;
    let work = u64::try_from(encoded.len())
        .ok()
        .and_then(|n| n.checked_add(128))
        .ok_or(CompletePolicyEncodingError::Schema(
            PolicySchemaError::LengthOverflow,
        ))?;
    charge_schema(charge, work, 0).map_err(CompletePolicyEncodingError::Schema)?;
    let mut copied = Vec::new();
    copied
        .try_reserve_exact(encoded.len())
        .map_err(|_| CompletePolicyEncodingError::AllocationFailure)?;
    copied.extend_from_slice(encoded);
    Ok(CompletePolicyValue {
        candidate,
        encoded: copied,
    })
}

/// Restores semantic owners without creating a second encoded envelope owner.
pub(super) fn decode_complete_policy_candidate_v2(
    encoded: &[u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyCandidate, CompletePolicyEncodingError> {
    decode_candidate(encoded, limits, charge, false)
}
fn decode_candidate(
    encoded: &[u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    profiled: bool,
) -> Result<CompletePolicyCandidate, CompletePolicyEncodingError> {
    if charge.profile() != limits {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch,
        ));
    }
    if encoded.len() > MAX_ENCODED_POLICY_BYTES {
        return Err(CompletePolicyEncodingError::ByteLimit);
    }
    let (generation, range) = if profiled {
        scan::scan_value_with_state_profile(encoded, limits, charge, true)?
    } else {
        scan::scan_value(encoded, limits, charge)?
    };
    let data = {
        let bytes = encoded
            .get(range)
            .ok_or(CompletePolicyEncodingError::Malformed)?;
        let mut reader = Reader::new(bytes, limits, charge);
        let data = if profiled {
            decode::decode_snapshot_with_state_profile(&mut reader, true)?
        } else {
            decode::decode_snapshot(&mut reader)?
        };
        reader.finish()?;
        data
    };
    let candidate = super::restore_complete_policy(generation, data, limits, charge)
        .map_err(CompletePolicyEncodingError::Schema)?;
    Ok(candidate)
}

fn current_error(error: PolicySchemaError) -> CompletePolicyEncodingError {
    CompletePolicyEncodingError::Schema(match error {
        error @ (PolicySchemaError::ProfileMismatch
        | PolicySchemaError::WorkBytes { .. }
        | PolicySchemaError::LengthOverflow) => error,
        _ => PolicySchemaError::InvalidAuthority,
    })
}

pub(super) fn verify_complete_policy_replace_current(
    value: &CompletePolicyValue,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), CompletePolicyEncodingError> {
    if charge.profile() != limits {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch,
        ));
    }
    if value.generation() == 0 || value.generation() > MAX_POLICY_GENERATION {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::InvalidAuthority,
        ));
    }
    if value.encoded().len() > MAX_ENCODED_POLICY_BYTES {
        return Err(CompletePolicyEncodingError::ByteLimit);
    }
    let measured = encode::measure_value(value.generation(), value.data(), limits, charge)?;
    encode::compare_value(&measured, value.encoded(), limits, charge)
}

pub(super) fn encode_complete_policy_replace_value(
    generation: u64,
    data: CompletePolicyData,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyValue, CompletePolicyEncodingError> {
    if charge.profile() != limits {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch,
        ));
    }
    if generation == 0 || generation > MAX_POLICY_GENERATION {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::InvalidAuthority,
        ));
    }
    let encoded = {
        let measured = encode::measure_value(generation, &data, limits, charge)?;
        encode::emit_value(&measured, limits, charge)?
    };
    Ok(CompletePolicyValue {
        candidate: CompletePolicyCandidate { generation, data },
        encoded,
    })
}
