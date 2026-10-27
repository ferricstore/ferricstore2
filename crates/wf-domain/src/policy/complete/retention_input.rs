//! Original optional retention validation and independently paid diagnostics.

use crate::policy::installation::PolicyError;
use crate::retention::{
    MAX_HISTORY_EVENTS, MAX_RETENTION_MILLIS, RetentionError, RetentionOverride,
};

use super::admission::charge_schema;
use super::flow_input::value::{PolicyInputCursor as Cursor, classify_map, select, unsigned};
use super::{
    CompletePolicyEncodingError as Error, OriginalPolicyMapKey as Key, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits, RawPolicyRetryValue as Raw,
};

/// Original caller diagnoses, separate from operational or retained authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyRetentionInputRejection {
    /// The input is not nil, a map or a classified keyword list.
    Shape,
    /// A supplied TTL is not a positive integer within the source range.
    Ttl,
    /// The internal hot-history key is present, regardless of its value.
    HistoryHot,
    /// A supplied history count is outside the original captured limit.
    History {
        /// The admitted immutable limit at the failed normalization.
        maximum: u32,
    },
}

/// Normalizes optional original retention using the existing raw vocabulary.
///
/// Nil stays absent and an empty map/list stays a present empty override. This
/// deliberately reuses the seven borrowed retry descriptor classes, without
/// granting parser, hidden-payload, map-identity or stored-policy authority.
/// Validation checks TTL before forbidden hot-key presence, then history.
/// No default merge, environment read, second business pass or allocation occurs.
///
/// # Errors
/// Outer errors preserve operational profile/context/work/visible-bound and
/// duplicate queried-key refusals. Inner errors are exact caller diagnoses.
pub fn normalize_complete_policy_retention_override_input(
    input: &Raw<'_>,
    history_max_events_limit: u32,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<Option<RetentionOverride>, PolicyRetentionInputRejection>, Error> {
    if charge.profile() != limits {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    valid_limit(history_max_events_limit)?;
    preflight(*input, 0, &mut (0, 0), charge)?;
    borrow_retention(Cursor::Retry(*input), history_max_events_limit, charge)
}

pub(in crate::policy::complete) fn borrow_retention(
    input: Cursor<'_>,
    history_max_events_limit: u32,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<Option<RetentionOverride>, PolicyRetentionInputRejection>, Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    if input.is_nil() {
        charge_schema(charge, 128, 0).map_err(Error::Schema)?;
        return Ok(Ok(None));
    }
    let Some(input) = classify_map(input, charge)? else {
        return Ok(Err(PolicyRetentionInputRejection::Shape));
    };
    charge_schema(charge, 0, 64).map_err(Error::Schema)?;
    let ttl_ms = if let Some(value) = select(input, b"ttl_ms", charge)? {
        charge_schema(charge, 128, 0).map_err(Error::Schema)?;
        let Some(value) = positive(value, MAX_RETENTION_MILLIS, charge)? else {
            return Ok(Err(PolicyRetentionInputRejection::Ttl));
        };
        Some(value)
    } else {
        None
    };
    if select(input, b"history_hot_max_events", charge)?.is_some() {
        return Ok(Err(PolicyRetentionInputRejection::HistoryHot));
    }
    let history_max_events = if let Some(value) = select(input, b"history_max_events", charge)? {
        charge_schema(charge, 128, 0).map_err(Error::Schema)?;
        let Some(value) = positive(value, u64::from(history_max_events_limit), charge)? else {
            return Ok(Err(PolicyRetentionInputRejection::History {
                maximum: history_max_events_limit,
            }));
        };
        Some(u32::try_from(value).map_err(|_| Error::Schema(PolicySchemaError::LengthOverflow))?)
    } else {
        None
    };
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    Ok(Ok(Some(RetentionOverride {
        ttl_ms,
        history_max_events,
    })))
}

/// Encodes one exact fixed or captured-limit caller diagnosis with fresh work.
///
/// The diagnostic is not proof of a parent branch. One fallible final Vec owns
/// its bytes; a local seven-byte decimal buffer borrows no input/context owner.
///
/// # Errors
/// Preserves profile, invalid diagnostic context, work and length errors, plus
/// selected reservation failure, with all earlier debits and peaks retained.
pub fn encode_complete_policy_retention_input_rejection_body(
    rejection: PolicyRetentionInputRejection,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<u8>, Error> {
    if charge.profile() != limits {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    let (prefix, maximum) = rejection_parts(rejection)?;
    let digits = maximum.map_or(0, decimal_length);
    let length = prefix
        .len()
        .checked_add(digits)
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    charge_schema(
        charge,
        u64::try_from(digits).map_err(|_| Error::Schema(PolicySchemaError::LengthOverflow))?,
        0,
    )
    .map_err(Error::Schema)?;
    charge_visit(length, charge)?;
    let mut scratch = [0; 7];
    let decimal = match maximum {
        Some(value) => write_decimal(value, &mut scratch)?,
        None => &[],
    };
    let mut owned = Vec::new();
    owned
        .try_reserve_exact(length)
        .map_err(|_| Error::AllocationFailure)?;
    owned.extend_from_slice(prefix);
    owned.extend_from_slice(decimal);
    Ok(owned)
}

pub(in crate::policy::complete) fn rejection_parts(
    rejection: PolicyRetentionInputRejection,
) -> Result<(&'static [u8], Option<u32>), Error> {
    Ok(match rejection {
        PolicyRetentionInputRejection::Shape => (
            b"ERR flow retention policy must be a map or keyword list".as_slice(),
            None,
        ),
        PolicyRetentionInputRejection::Ttl => (
            b"ERR flow retention ttl_ms must be between 1 and 31536000000".as_slice(),
            None,
        ),
        PolicyRetentionInputRejection::HistoryHot => (
            b"ERR flow retention history_hot_max_events is internal".as_slice(),
            None,
        ),
        PolicyRetentionInputRejection::History { maximum } => {
            valid_limit(maximum)?;
            (
                b"ERR flow retention history_max_events must be between 1 and ".as_slice(),
                Some(maximum),
            )
        }
    })
}

pub(in crate::policy::complete) fn valid_limit(maximum: u32) -> Result<(), Error> {
    if maximum == 0 || maximum > MAX_HISTORY_EVENTS {
        return Err(Error::Schema(PolicySchemaError::InvalidAuthority));
    }
    Ok(())
}

fn positive(
    value: Cursor<'_>,
    maximum: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<u64>, Error> {
    Ok(unsigned(value, maximum, charge)?.filter(|&n| n != 0))
}

fn preflight(
    input: Raw<'_>,
    depth: usize,
    measured: &mut (u64, u64),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    visit_descriptor(depth, &mut measured.0, charge)?;
    match input {
        Raw::Atom(text) => text_bytes(text.len(), measured, charge),
        Raw::Binary(bytes) => text_bytes(bytes.len(), measured, charge),
        Raw::Map(rows) => {
            let mut queried = 0_u8;
            for &(key, value) in rows {
                preflight_key(key, depth + 1, measured, &mut queried, true, charge)?;
                preflight(value, depth + 1, measured, charge)?;
            }
            Ok(())
        }
        Raw::Keyword(rows) => {
            let mut queried = 0_u8;
            for &(key, value) in rows {
                preflight_key(
                    Key::Atom(key),
                    depth + 1,
                    measured,
                    &mut queried,
                    false,
                    charge,
                )?;
                preflight(value, depth + 1, measured, charge)?;
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
    queried: &mut u8,
    unique: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    visit_descriptor(depth, &mut measured.0, charge)?;
    if let Some(bytes) = key_bytes(key) {
        text_bytes(bytes.len(), measured, charge)?;
        for (index, name) in [
            b"ttl_ms".as_slice(),
            b"history_hot_max_events",
            b"history_max_events",
        ]
        .into_iter()
        .enumerate()
        {
            let length = bytes
                .len()
                .checked_add(name.len())
                .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
            charge_visit(length, charge)?;
            if bytes != name {
                continue;
            }
            charge_schema(charge, 128, 0).map_err(Error::Schema)?;
            let bit = 1 << (index + usize::from(matches!(key, Key::Binary(_))) * 3);
            if unique && *queried & bit != 0 {
                return Err(Error::Malformed);
            }
            *queried |= bit;
            break;
        }
    }
    Ok(())
}

fn key_bytes(key: Key<'_>) -> Option<&[u8]> {
    match key {
        Key::Atom(text) => Some(text.as_bytes()),
        Key::Binary(bytes) => Some(bytes),
        Key::Other => None,
    }
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

fn charge_visit(length: usize, charge: &mut PolicySchemaCharge) -> Result<(), Error> {
    let work = u64::try_from(length)
        .ok()
        .and_then(|n| n.checked_add(128))
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    charge_schema(charge, work, 0).map_err(Error::Schema)
}

pub(in crate::policy::complete) fn decimal_length(mut value: u32) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

pub(in crate::policy::complete) fn write_decimal(
    mut value: u32,
    scratch: &mut [u8; 7],
) -> Result<&[u8], Error> {
    let mut start = scratch.len();
    while value != 0 {
        start = start
            .checked_sub(1)
            .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
        let digit = u8::try_from(value % 10)
            .map_err(|_| Error::Schema(PolicySchemaError::LengthOverflow))?;
        scratch[start] = b'0' + digit;
        value /= 10;
    }
    Ok(&scratch[start..])
}

pub(in crate::policy::complete) fn valid_retention(
    field: usize,
    value: i128,
    maximum: u32,
) -> Result<(), Error> {
    let bound = if field == 0 {
        31_536_000_000
    } else {
        i128::from(maximum)
    };
    if !(1..=bound).contains(&value) {
        return Err(Error::Schema(PolicySchemaError::Policy(
            PolicyError::Retention(if field == 0 {
                RetentionError::Ttl
            } else {
                RetentionError::History
            }),
        )));
    }
    Ok(())
}
