//! Bounded original retry input normalization and final target ownership.

use super::original_entry::indexed::OriginalPolicyCurrentIndex as Index;
use crate::policy::installation::PolicyError;
use crate::policy::{Backoff, RetryOverride, RetryPolicy};
use crate::value::{DomainError, RetryPolicyField};

use super::admission::charge_schema;
use super::{
    CompletePolicyEncodingError as Error, OriginalPolicyMapKey as Key, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits,
};

mod value;

use super::flow_input::value::{PolicyInputCursor as Cursor, classify_map, select};

/// Borrowed original retry input classes, without raw-parser authority.
///
/// `Keyword` supplies an already-classified proper atom-key pair list. `Other`
/// hides its original term payload, so the bounds cover visible descriptors only.
/// Atom `"nil"` aliases `Nil` semantically, while its text is still prepaid.
#[derive(Clone, Copy, Debug)]
pub enum RawPolicyRetryValue<'input> {
    /// The original nil atom.
    Nil,
    /// An exact represented integer; unsupported numeric classes use `Other`.
    Integer(i128),
    /// An atom spelling, without atom allocation or interning.
    Atom(&'input str),
    /// Original binary bytes, without UTF-8 or trimming requirements.
    Binary(&'input [u8]),
    /// Map entries in supplied traversal order; queried identities are unique.
    Map(&'input [(Key<'input>, Self)]),
    /// Proper atom-key pairs, with source last-duplicate semantics.
    Keyword(&'input [(&'input str, Self)]),
    /// An unsupported reached class, with unrepresented term payload.
    Other,
}

/// Fixed caller diagnostics, separate from operational encoding refusals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyRetryInputRejection {
    /// The root is not a map, keyword list or nil.
    Shape,
    /// The old `max_attempts` spelling is present, regardless of its value.
    DeprecatedMaxAttempts,
    /// The retry count is not an integer in the source range.
    MaxRetries,
    /// A supplied backoff is not a map, keyword list or nil.
    BackoffShape,
    /// The kind is not one of the four exact source spellings.
    BackoffKind,
    /// The base delay is outside the source range or integer class.
    BaseMillis,
    /// The maximum delay is outside the source range or integer class.
    MaxMillis,
    /// The jitter percentage is outside the source range or integer class.
    JitterPercent,
    /// The target is not a nonempty binary.
    ExhaustedTo,
    /// The target is exactly the reserved active state.
    RunningExhaustion,
}

impl PolicyRetryInputRejection {
    /// Returns the exact fixed source response, without allocating.
    #[must_use]
    pub const fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::Shape => b"ERR flow retry policy must be a map or keyword list",
            Self::DeprecatedMaxAttempts => {
                b"ERR flow retry max_attempts was renamed to max_retries"
            }
            Self::MaxRetries => b"ERR flow retry max_retries must be between 0 and 1000",
            Self::BackoffShape => b"ERR flow retry backoff must be a map or keyword list",
            Self::BackoffKind => {
                b"ERR flow retry backoff kind must be none, fixed, linear, or exponential"
            }
            Self::BaseMillis => b"ERR flow retry base_ms must be between 0 and 2592000000",
            Self::MaxMillis => b"ERR flow retry max_ms must be between 0 and 2592000000",
            Self::JitterPercent => b"ERR flow retry jitter_pct must be between 0 and 100",
            Self::ExhaustedTo => b"ERR flow retry exhausted_to must be a non-empty string",
            Self::RunningExhaustion => b"ERR flow retry exhausted_to cannot be running",
        }
    }
}

/// Normalizes a full original retry value into the existing owned policy.
///
/// All visible input is bounded before source-order validation. A final target
/// copy occurs only after every semantic field succeeds. This is no ETF-size,
/// stored-policy, retained-outcome or public ingress authority.
///
/// # Errors
/// The outer result preserves profile, work, descriptor, duplicate queried-key
/// and allocation refusals. The inner result contains exact caller diagnostics.
pub fn normalize_complete_policy_retry_input(
    input: &RawPolicyRetryValue<'_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<RetryPolicy, PolicyRetryInputRejection>, Error> {
    let fields = match normalize_fields(input, true, limits, charge)? {
        Ok(fields) => fields,
        Err(reason) => return Ok(Err(reason)),
    };
    let Some(RetryOverride {
        max_retries: Some(max_retries),
        backoff: Some(backoff),
        exhausted_to: Some(exhausted_to),
    }) = fields
    else {
        return Err(Error::Malformed);
    };
    Ok(Ok(RetryPolicy {
        max_retries,
        backoff,
        exhausted_to,
    }))
}

/// Normalizes original override presence without filling absent root fields.
///
/// Nil returns `None`; a supplied empty collection returns an empty override.
/// Any supplied backoff, including nil, fills all four backoff defaults.
///
/// # Errors
/// Preserves operational refusals outside the inner fixed caller diagnostics.
pub fn normalize_complete_policy_retry_override_input(
    input: &RawPolicyRetryValue<'_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<Option<RetryOverride>, PolicyRetryInputRejection>, Error> {
    normalize_fields(input, false, limits, charge)
}

fn normalize_fields(
    input: &RawPolicyRetryValue<'_>,
    fill_defaults: bool,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<Option<RetryOverride>, PolicyRetryInputRejection>, Error> {
    if charge.profile() != limits {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    preflight(*input, 0, &mut (0, 0), charge)?;
    let fields = match borrow_retry(Cursor::Retry(*input), fill_defaults, charge)? {
        Ok(fields) => fields,
        Err(reason) => return Ok(Err(reason)),
    };
    Ok(Ok(fields
        .map(|fields| {
            Ok(RetryOverride {
                max_retries: fields.max_retries,
                backoff: fields.backoff,
                exhausted_to: fields
                    .exhausted_to
                    .map(|bytes| copy_target(bytes, charge))
                    .transpose()?,
            })
        })
        .transpose()?))
}

pub(in crate::policy::complete) struct BorrowedPolicyRetry<'input> {
    pub(in crate::policy::complete) max_retries: Option<u16>,
    pub(in crate::policy::complete) backoff: Option<Backoff>,
    pub(in crate::policy::complete) exhausted_to: Option<&'input [u8]>,
}

pub(in crate::policy::complete) fn borrow_retry<'input>(
    input: Cursor<'input>,
    fill_defaults: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<Option<BorrowedPolicyRetry<'input>>, PolicyRetryInputRejection>, Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    if input.is_nil() {
        charge_schema(charge, 128, 0).map_err(Error::Schema)?;
        return Ok(Ok(fill_defaults.then_some(BorrowedPolicyRetry {
            max_retries: Some(3),
            backoff: Some(Backoff::default()),
            exhausted_to: Some(b"failed"),
        })));
    }
    let Some(input) = classify_map(input, charge)? else {
        return Ok(Err(PolicyRetryInputRejection::Shape));
    };
    charge_schema(charge, 0, 128).map_err(Error::Schema)?;
    if select(input, b"max_attempts", charge)?.is_some() {
        return Ok(Err(PolicyRetryInputRejection::DeprecatedMaxAttempts));
    }
    let selected = select(input, b"max_retries", charge)?;
    let max_retries = match value::retry_limit(selected, fill_defaults, charge)? {
        Ok(maximum) => maximum,
        Err(reason) => return Ok(Err(reason)),
    };
    let selected = select(input, b"backoff", charge)?;
    let backoff = match value::backoff(selected, fill_defaults, charge)? {
        Ok(backoff) => backoff,
        Err(reason) => return Ok(Err(reason)),
    };
    let selected = select(input, b"exhausted_to", charge)?;
    let target = match value::target(selected, fill_defaults, charge)? {
        Ok(target) => target,
        Err(reason) => return Ok(Err(reason)),
    };
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    Ok(Ok(Some(BorrowedPolicyRetry {
        max_retries,
        backoff,
        exhausted_to: target,
    })))
}

fn preflight(
    input: RawPolicyRetryValue<'_>,
    depth: usize,
    measured: &mut (u64, u64),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    visit_descriptor(depth, &mut measured.0, charge)?;
    match input {
        RawPolicyRetryValue::Atom(text) => text_bytes(text.len(), measured, charge),
        RawPolicyRetryValue::Binary(bytes) => text_bytes(bytes.len(), measured, charge),
        RawPolicyRetryValue::Map(rows) => {
            let mut queried = 0_u16;
            for &(key, child) in rows {
                preflight_key(key, depth + 1, measured, &mut queried, true, charge)?;
                preflight(child, depth + 1, measured, charge)?;
            }
            Ok(())
        }
        RawPolicyRetryValue::Keyword(rows) => {
            let mut queried = 0_u16;
            for &(key, child) in rows {
                preflight_key(
                    Key::Atom(key),
                    depth + 1,
                    measured,
                    &mut queried,
                    false,
                    charge,
                )?;
                preflight(child, depth + 1, measured, charge)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn preflight_key(
    key: Key<'_>,
    depth: usize,
    measured: &mut (u64, u64),
    queried: &mut u16,
    unique: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    visit_descriptor(depth, &mut measured.0, charge)?;
    if let Some(bytes) = value::key_bytes(key) {
        text_bytes(bytes.len(), measured, charge)?;
        if let Some(bit) = value::queried_key(key, charge)? {
            charge_schema(charge, 128, 0).map_err(Error::Schema)?;
            if unique && *queried & bit != 0 {
                return Err(Error::Malformed);
            }
            *queried |= bit;
        }
    }
    Ok(())
}

fn visit_descriptor(
    depth: usize,
    count: &mut u64,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    *count = count
        .checked_add(1)
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    if depth > 64 || *count > 4_096 {
        return Err(Error::ItemLimit);
    }
    Ok(())
}

fn text_bytes(
    length: usize,
    measured: &mut (u64, u64),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let length =
        u64::try_from(length).map_err(|_| Error::Schema(PolicySchemaError::LengthOverflow))?;
    measured.1 = measured
        .1
        .checked_add(length)
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    if measured.1 > 262_144 {
        return Err(Error::ByteLimit);
    }
    charge_schema(charge, length, 0).map_err(Error::Schema)
}

fn copy_target(bytes: &[u8], charge: &mut PolicySchemaCharge) -> Result<Vec<u8>, Error> {
    value::charge_visit(bytes.len(), charge)?;
    let mut owned = Vec::new();
    owned
        .try_reserve_exact(bytes.len())
        .map_err(|_| Error::AllocationFailure)?;
    owned.extend_from_slice(bytes);
    Ok(owned)
}

pub(in crate::policy::complete) fn number(
    index: &Index<'_, 'static>,
    position: u32,
    maximum: u64,
    field: RetryPolicyField,
) -> Result<u64, Error> {
    index
        .raw_cursor(position)
        .integer()
        .and_then(|n| u64::try_from(n).ok())
        .filter(|&n| n <= maximum)
        .ok_or_else(|| invalid(field))
}
pub(in crate::policy::complete) fn invalid(field: RetryPolicyField) -> Error {
    domain(DomainError::InvalidRetryPolicy(field))
}
pub(in crate::policy::complete) fn domain(error: DomainError) -> Error {
    Error::Schema(PolicySchemaError::Policy(PolicyError::Domain(error)))
}

pub(in crate::policy::complete) fn raw_backoff_kind(
    index: &Index<'_, 'static>,
    position: u32,
    charge: &mut PolicySchemaCharge,
) -> Result<crate::policy::BackoffKind, Error> {
    use crate::policy::BackoffKind;
    let cursor = index.raw_cursor(position);
    if cursor.binary_bytes().is_some() {
        return value::kind(Some(Cursor::Current(cursor)), charge)?
            .map_err(|_| invalid(RetryPolicyField::BaseMillis));
    }
    match cursor.atom_bytes() {
        Some(b"none") => Ok(BackoffKind::None),
        Some(b"fixed") => Ok(BackoffKind::Fixed),
        Some(b"linear") => Ok(BackoffKind::Linear),
        Some(b"exponential") => Ok(BackoffKind::Exponential),
        _ => Err(invalid(RetryPolicyField::BaseMillis)),
    }
}
